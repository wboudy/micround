//! Latency Measurement Tooling
//!
//! Provides end-to-end latency tracking from camera capture to display presentation.
//! Essential for validating the <100ms latency requirement.
//!
//! # Architecture
//!
//! ```text
//! Camera → Decode → Process → Render → Present
//!   t0       t1       t2        t3       t4
//! ```
//!
//! Each stage records its completion timestamp, allowing precise measurement
//! of where time is spent in the pipeline.
//!
//! # Usage
//!
//! ```ignore
//! use micround::core::latency::{FrameMetrics, LatencyTracker};
//!
//! // Create tracker
//! let tracker = LatencyTracker::new();
//!
//! // Start tracking a frame
//! let mut metrics = FrameMetrics::new(frame_sequence, capture_timestamp);
//!
//! // Record stage completions
//! metrics.mark_decode_complete();
//! metrics.mark_process_complete();
//! metrics.mark_render_complete();
//! metrics.mark_present_complete();
//!
//! // Submit to tracker
//! tracker.submit(metrics);
//!
//! // Get statistics
//! let histogram = tracker.histogram();
//! println!("p95 latency: {}ms", histogram.percentile(95));
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Maximum number of frame metrics to retain for analysis
const MAX_RETAINED_METRICS: usize = 1000;

/// Latency histogram buckets (in milliseconds)
const BUCKET_BOUNDARIES: [u64; 6] = [20, 40, 60, 80, 100, u64::MAX];

/// Per-frame timing metrics through the processing pipeline
#[derive(Debug, Clone)]
pub struct FrameMetrics {
    /// Frame sequence number for correlation
    pub sequence: u64,
    /// Timestamp when frame was captured (from camera)
    pub capture_time: Instant,
    /// Timestamp when decode completed (None if raw format)
    pub decode_time: Option<Instant>,
    /// Timestamp when processing completed
    pub process_time: Option<Instant>,
    /// Timestamp when submitted to renderer
    pub render_time: Option<Instant>,
    /// Timestamp when presented to display
    pub present_time: Option<Instant>,
}

impl FrameMetrics {
    /// Create new metrics starting from capture time
    pub fn new(sequence: u64, capture_time: Instant) -> Self {
        Self {
            sequence,
            capture_time,
            decode_time: None,
            process_time: None,
            render_time: None,
            present_time: None,
        }
    }

    /// Create new metrics using current time as capture time
    pub fn new_now(sequence: u64) -> Self {
        Self::new(sequence, Instant::now())
    }

    /// Mark decode stage as complete
    pub fn mark_decode_complete(&mut self) {
        self.decode_time = Some(Instant::now());
    }

    /// Mark processing stage as complete
    pub fn mark_process_complete(&mut self) {
        self.process_time = Some(Instant::now());
    }

    /// Mark render submission as complete
    pub fn mark_render_complete(&mut self) {
        self.render_time = Some(Instant::now());
    }

    /// Mark display presentation as complete
    pub fn mark_present_complete(&mut self) {
        self.present_time = Some(Instant::now());
    }

    /// Get decode latency in microseconds (None if decode was skipped)
    pub fn decode_latency_us(&self) -> Option<u64> {
        self.decode_time.map(|t| t.duration_since(self.capture_time).as_micros() as u64)
    }

    /// Get processing latency in microseconds (from decode/capture to process complete)
    pub fn process_latency_us(&self) -> Option<u64> {
        let start = self.decode_time.unwrap_or(self.capture_time);
        self.process_time.map(|t| t.duration_since(start).as_micros() as u64)
    }

    /// Get render latency in microseconds (from process to render submit)
    pub fn render_latency_us(&self) -> Option<u64> {
        let start = self.process_time?;
        self.render_time.map(|t| t.duration_since(start).as_micros() as u64)
    }

    /// Get present latency in microseconds (from render to present)
    pub fn present_latency_us(&self) -> Option<u64> {
        let start = self.render_time?;
        self.present_time.map(|t| t.duration_since(start).as_micros() as u64)
    }

    /// Get total end-to-end latency in microseconds
    /// (from capture to present, or to last completed stage)
    pub fn total_latency_us(&self) -> u64 {
        let end = self.present_time
            .or(self.render_time)
            .or(self.process_time)
            .or(self.decode_time)
            .unwrap_or(self.capture_time);
        end.duration_since(self.capture_time).as_micros() as u64
    }

    /// Get total end-to-end latency in milliseconds
    pub fn total_latency_ms(&self) -> f64 {
        self.total_latency_us() as f64 / 1000.0
    }

    /// Check if frame met the latency target
    pub fn meets_target(&self, target_ms: f64) -> bool {
        self.total_latency_ms() <= target_ms
    }

    /// Get a breakdown of time spent in each stage
    pub fn stage_breakdown(&self) -> StageBreakdown {
        StageBreakdown {
            decode_us: self.decode_latency_us(),
            process_us: self.process_latency_us(),
            render_us: self.render_latency_us(),
            present_us: self.present_latency_us(),
            total_us: self.total_latency_us(),
        }
    }
}

/// Breakdown of time spent in each pipeline stage
#[derive(Debug, Clone, Default)]
pub struct StageBreakdown {
    /// Decode stage latency in microseconds
    pub decode_us: Option<u64>,
    /// Process stage latency in microseconds
    pub process_us: Option<u64>,
    /// Render stage latency in microseconds
    pub render_us: Option<u64>,
    /// Present stage latency in microseconds
    pub present_us: Option<u64>,
    /// Total end-to-end latency in microseconds
    pub total_us: u64,
}

impl StageBreakdown {
    /// Get total as milliseconds
    pub fn total_ms(&self) -> f64 {
        self.total_us as f64 / 1000.0
    }
}

/// Latency histogram for distribution analysis
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    /// Bucket counts: [0-20ms, 20-40ms, 40-60ms, 60-80ms, 80-100ms, 100ms+]
    buckets: [u64; 6],
    /// Total samples
    count: u64,
    /// Sum of all latencies (for mean calculation)
    sum_us: u64,
    /// Minimum observed latency
    min_us: u64,
    /// Maximum observed latency
    max_us: u64,
    /// Sorted latencies for percentile calculation (limited size)
    samples: Vec<u64>,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    /// Create a new empty histogram
    pub fn new() -> Self {
        Self {
            buckets: [0; 6],
            count: 0,
            sum_us: 0,
            min_us: u64::MAX,
            max_us: 0,
            samples: Vec::new(),
        }
    }

    /// Record a latency sample (in microseconds)
    pub fn record(&mut self, latency_us: u64) {
        let latency_ms = latency_us / 1000;

        // Update bucket
        let bucket_idx = BUCKET_BOUNDARIES.iter()
            .position(|&boundary| latency_ms < boundary)
            .unwrap_or(5);
        self.buckets[bucket_idx] += 1;

        // Update stats
        self.count += 1;
        self.sum_us += latency_us;
        self.min_us = self.min_us.min(latency_us);
        self.max_us = self.max_us.max(latency_us);

        // Store sample for percentile calculation (reservoir sampling for large sets)
        if self.samples.len() < MAX_RETAINED_METRICS {
            self.samples.push(latency_us);
        }
    }

    /// Record a latency sample (in milliseconds)
    pub fn record_ms(&mut self, latency_ms: f64) {
        self.record((latency_ms * 1000.0) as u64);
    }

    /// Get the count of samples
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Get the mean latency in milliseconds
    pub fn mean_ms(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        (self.sum_us as f64 / self.count as f64) / 1000.0
    }

    /// Get the minimum latency in milliseconds
    pub fn min_ms(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.min_us as f64 / 1000.0
    }

    /// Get the maximum latency in milliseconds
    pub fn max_ms(&self) -> f64 {
        self.max_us as f64 / 1000.0
    }

    /// Get the specified percentile (0-100) in milliseconds
    pub fn percentile(&self, p: u8) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }

        let mut sorted = self.samples.clone();
        sorted.sort_unstable();

        let idx = ((p as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx] as f64 / 1000.0
    }

    /// Get p50 (median) latency in milliseconds
    pub fn p50(&self) -> f64 {
        self.percentile(50)
    }

    /// Get p95 latency in milliseconds
    pub fn p95(&self) -> f64 {
        self.percentile(95)
    }

    /// Get p99 latency in milliseconds
    pub fn p99(&self) -> f64 {
        self.percentile(99)
    }

    /// Get bucket distribution as percentages
    pub fn bucket_percentages(&self) -> [f64; 6] {
        if self.count == 0 {
            return [0.0; 6];
        }
        let count = self.count as f64;
        [
            self.buckets[0] as f64 / count * 100.0,
            self.buckets[1] as f64 / count * 100.0,
            self.buckets[2] as f64 / count * 100.0,
            self.buckets[3] as f64 / count * 100.0,
            self.buckets[4] as f64 / count * 100.0,
            self.buckets[5] as f64 / count * 100.0,
        ]
    }

    /// Get bucket labels
    pub fn bucket_labels() -> [&'static str; 6] {
        ["0-20ms", "20-40ms", "40-60ms", "60-80ms", "80-100ms", "100ms+"]
    }

    /// Check if the latency meets targets
    pub fn meets_targets(&self, p50_target_ms: f64, p95_target_ms: f64) -> bool {
        self.p50() <= p50_target_ms && self.p95() <= p95_target_ms
    }

    /// Generate a summary report string
    pub fn summary(&self) -> String {
        format!(
            "Latency: count={}, min={:.1}ms, mean={:.1}ms, p50={:.1}ms, p95={:.1}ms, p99={:.1}ms, max={:.1}ms",
            self.count,
            self.min_ms(),
            self.mean_ms(),
            self.p50(),
            self.p95(),
            self.p99(),
            self.max_ms()
        )
    }

    /// Merge another histogram into this one
    pub fn merge(&mut self, other: &LatencyHistogram) {
        for (i, count) in other.buckets.iter().enumerate() {
            self.buckets[i] += count;
        }
        self.count += other.count;
        self.sum_us += other.sum_us;
        self.min_us = self.min_us.min(other.min_us);
        self.max_us = self.max_us.max(other.max_us);

        // Merge samples (keeping within limit)
        for &sample in &other.samples {
            if self.samples.len() < MAX_RETAINED_METRICS {
                self.samples.push(sample);
            }
        }
    }

    /// Reset the histogram
    pub fn reset(&mut self) {
        self.buckets = [0; 6];
        self.count = 0;
        self.sum_us = 0;
        self.min_us = u64::MAX;
        self.max_us = 0;
        self.samples.clear();
    }
}

/// Thread-safe latency tracker for collecting metrics across the pipeline
pub struct LatencyTracker {
    /// Recent frame metrics for analysis
    recent_metrics: RwLock<VecDeque<FrameMetrics>>,
    /// Running histogram
    histogram: RwLock<LatencyHistogram>,
    /// Total frames tracked
    frames_tracked: AtomicU64,
    /// Frames that met the latency target
    frames_met_target: AtomicU64,
    /// Latency target in milliseconds
    target_ms: f64,
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyTracker {
    /// Create a new latency tracker with default 100ms target
    pub fn new() -> Self {
        Self::with_target(100.0)
    }

    /// Create a new latency tracker with custom target
    pub fn with_target(target_ms: f64) -> Self {
        Self {
            recent_metrics: RwLock::new(VecDeque::with_capacity(MAX_RETAINED_METRICS)),
            histogram: RwLock::new(LatencyHistogram::new()),
            frames_tracked: AtomicU64::new(0),
            frames_met_target: AtomicU64::new(0),
            target_ms,
        }
    }

    /// Submit frame metrics for tracking
    pub fn submit(&self, metrics: FrameMetrics) {
        let latency_us = metrics.total_latency_us();
        let met_target = metrics.meets_target(self.target_ms);

        // Update histogram
        if let Ok(mut hist) = self.histogram.write() {
            hist.record(latency_us);
        }

        // Store recent metrics
        if let Ok(mut recent) = self.recent_metrics.write() {
            if recent.len() >= MAX_RETAINED_METRICS {
                recent.pop_front();
            }
            recent.push_back(metrics);
        }

        // Update counters
        self.frames_tracked.fetch_add(1, Ordering::Relaxed);
        if met_target {
            self.frames_met_target.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get a copy of the current histogram
    pub fn histogram(&self) -> LatencyHistogram {
        self.histogram.read()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// Get recent frame metrics (up to count)
    pub fn recent_metrics(&self, count: usize) -> Vec<FrameMetrics> {
        self.recent_metrics.read()
            .map(|recent| {
                recent.iter()
                    .rev()
                    .take(count)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get total frames tracked
    pub fn frames_tracked(&self) -> u64 {
        self.frames_tracked.load(Ordering::Relaxed)
    }

    /// Get percentage of frames meeting target
    pub fn target_hit_rate(&self) -> f64 {
        let tracked = self.frames_tracked.load(Ordering::Relaxed);
        if tracked == 0 {
            return 100.0;
        }
        let met = self.frames_met_target.load(Ordering::Relaxed);
        (met as f64 / tracked as f64) * 100.0
    }

    /// Get the latency target
    pub fn target_ms(&self) -> f64 {
        self.target_ms
    }

    /// Generate a summary report
    pub fn summary_report(&self) -> LatencySummaryReport {
        let histogram = self.histogram();
        LatencySummaryReport {
            frames_tracked: self.frames_tracked(),
            target_ms: self.target_ms,
            target_hit_rate: self.target_hit_rate(),
            min_ms: histogram.min_ms(),
            mean_ms: histogram.mean_ms(),
            p50_ms: histogram.p50(),
            p95_ms: histogram.p95(),
            p99_ms: histogram.p99(),
            max_ms: histogram.max_ms(),
            bucket_percentages: histogram.bucket_percentages(),
        }
    }

    /// Reset all tracking data
    pub fn reset(&self) {
        if let Ok(mut recent) = self.recent_metrics.write() {
            recent.clear();
        }
        if let Ok(mut hist) = self.histogram.write() {
            hist.reset();
        }
        self.frames_tracked.store(0, Ordering::Relaxed);
        self.frames_met_target.store(0, Ordering::Relaxed);
    }
}

/// Summary report of latency tracking
#[derive(Debug, Clone)]
pub struct LatencySummaryReport {
    /// Total frames tracked
    pub frames_tracked: u64,
    /// Target latency in ms
    pub target_ms: f64,
    /// Percentage of frames meeting target
    pub target_hit_rate: f64,
    /// Minimum latency in ms
    pub min_ms: f64,
    /// Mean latency in ms
    pub mean_ms: f64,
    /// p50 latency in ms
    pub p50_ms: f64,
    /// p95 latency in ms
    pub p95_ms: f64,
    /// p99 latency in ms
    pub p99_ms: f64,
    /// Maximum latency in ms
    pub max_ms: f64,
    /// Bucket distribution percentages
    pub bucket_percentages: [f64; 6],
}

impl LatencySummaryReport {
    /// Check if latency meets acceptance criteria
    pub fn passes_acceptance(&self) -> bool {
        self.p50_ms < 80.0 && self.p95_ms < 100.0
    }

    /// Format as a string report
    pub fn to_string_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Latency Summary Report ===\n");
        report.push_str(&format!("Frames tracked: {}\n", self.frames_tracked));
        report.push_str(&format!("Target: {}ms (hit rate: {:.1}%)\n", self.target_ms, self.target_hit_rate));
        report.push_str("\nPercentiles:\n");
        report.push_str(&format!("  Min:  {:.2}ms\n", self.min_ms));
        report.push_str(&format!("  Mean: {:.2}ms\n", self.mean_ms));
        report.push_str(&format!("  p50:  {:.2}ms\n", self.p50_ms));
        report.push_str(&format!("  p95:  {:.2}ms\n", self.p95_ms));
        report.push_str(&format!("  p99:  {:.2}ms\n", self.p99_ms));
        report.push_str(&format!("  Max:  {:.2}ms\n", self.max_ms));
        report.push_str("\nDistribution:\n");
        let labels = LatencyHistogram::bucket_labels();
        for (i, (label, pct)) in labels.iter().zip(self.bucket_percentages.iter()).enumerate() {
            let bar_len = (*pct / 5.0).round() as usize;
            let bar: String = "█".repeat(bar_len);
            report.push_str(&format!("  {:>10}: {:>5.1}% {}\n", label, pct, bar));
        }
        report.push_str(&format!("\nAcceptance: {}\n",
            if self.passes_acceptance() { "PASS ✓" } else { "FAIL ✗" }
        ));
        report
    }
}

/// Shared latency tracker that can be cloned and used across threads
pub type SharedLatencyTracker = Arc<LatencyTracker>;

/// Create a shared latency tracker
pub fn shared_latency_tracker() -> SharedLatencyTracker {
    Arc::new(LatencyTracker::new())
}

/// Create a shared latency tracker with custom target
pub fn shared_latency_tracker_with_target(target_ms: f64) -> SharedLatencyTracker {
    Arc::new(LatencyTracker::with_target(target_ms))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_frame_metrics_creation() {
        let metrics = FrameMetrics::new_now(42);
        assert_eq!(metrics.sequence, 42);
        assert!(metrics.decode_time.is_none());
        assert!(metrics.process_time.is_none());
    }

    #[test]
    fn test_frame_metrics_stages() {
        let mut metrics = FrameMetrics::new_now(1);

        thread::sleep(Duration::from_millis(5));
        metrics.mark_decode_complete();
        assert!(metrics.decode_time.is_some());

        thread::sleep(Duration::from_millis(5));
        metrics.mark_process_complete();
        assert!(metrics.process_time.is_some());

        thread::sleep(Duration::from_millis(5));
        metrics.mark_render_complete();
        assert!(metrics.render_time.is_some());

        thread::sleep(Duration::from_millis(5));
        metrics.mark_present_complete();
        assert!(metrics.present_time.is_some());

        // Total should be >= 20ms (4 * 5ms sleeps)
        assert!(metrics.total_latency_ms() >= 20.0);
    }

    #[test]
    fn test_frame_metrics_breakdown() {
        let mut metrics = FrameMetrics::new_now(1);
        metrics.mark_decode_complete();
        metrics.mark_process_complete();

        let breakdown = metrics.stage_breakdown();
        assert!(breakdown.decode_us.is_some());
        assert!(breakdown.process_us.is_some());
        assert!(breakdown.render_us.is_none());
        assert!(breakdown.present_us.is_none());
    }

    #[test]
    fn test_histogram_empty() {
        let hist = LatencyHistogram::new();
        assert_eq!(hist.count(), 0);
        assert_eq!(hist.mean_ms(), 0.0);
        assert_eq!(hist.p50(), 0.0);
    }

    #[test]
    fn test_histogram_single_sample() {
        let mut hist = LatencyHistogram::new();
        hist.record(50_000); // 50ms in microseconds

        assert_eq!(hist.count(), 1);
        assert_eq!(hist.min_ms(), 50.0);
        assert_eq!(hist.max_ms(), 50.0);
        assert_eq!(hist.mean_ms(), 50.0);
        assert_eq!(hist.p50(), 50.0);
    }

    #[test]
    fn test_histogram_buckets() {
        let mut hist = LatencyHistogram::new();

        // Add samples in different buckets
        hist.record(10_000);  // 10ms -> bucket 0 (0-20)
        hist.record(30_000);  // 30ms -> bucket 1 (20-40)
        hist.record(50_000);  // 50ms -> bucket 2 (40-60)
        hist.record(70_000);  // 70ms -> bucket 3 (60-80)
        hist.record(90_000);  // 90ms -> bucket 4 (80-100)
        hist.record(150_000); // 150ms -> bucket 5 (100+)

        assert_eq!(hist.count(), 6);

        let pcts = hist.bucket_percentages();
        // Each bucket should have ~16.67%
        for pct in &pcts {
            assert!((*pct - 16.67).abs() < 1.0);
        }
    }

    #[test]
    fn test_histogram_percentiles() {
        let mut hist = LatencyHistogram::new();

        // Add 100 samples: 1ms, 2ms, ..., 100ms
        for i in 1..=100 {
            hist.record(i * 1000);
        }

        assert_eq!(hist.count(), 100);
        assert_eq!(hist.min_ms(), 1.0);
        assert_eq!(hist.max_ms(), 100.0);

        // p50 should be around 50ms
        let p50 = hist.p50();
        assert!(p50 >= 49.0 && p50 <= 51.0, "p50={}", p50);

        // p95 should be around 95ms
        let p95 = hist.p95();
        assert!(p95 >= 94.0 && p95 <= 96.0, "p95={}", p95);
    }

    #[test]
    fn test_histogram_merge() {
        let mut hist1 = LatencyHistogram::new();
        hist1.record(10_000);
        hist1.record(20_000);

        let mut hist2 = LatencyHistogram::new();
        hist2.record(30_000);
        hist2.record(40_000);

        hist1.merge(&hist2);

        assert_eq!(hist1.count(), 4);
        assert_eq!(hist1.min_ms(), 10.0);
        assert_eq!(hist1.max_ms(), 40.0);
    }

    #[test]
    fn test_tracker_basic() {
        let tracker = LatencyTracker::new();

        let mut metrics = FrameMetrics::new_now(1);
        thread::sleep(Duration::from_millis(10));
        metrics.mark_present_complete();

        tracker.submit(metrics);

        assert_eq!(tracker.frames_tracked(), 1);

        let hist = tracker.histogram();
        assert_eq!(hist.count(), 1);
    }

    #[test]
    fn test_tracker_target_hit_rate() {
        let tracker = LatencyTracker::with_target(50.0);

        // Submit a fast frame (< 50ms)
        let mut fast = FrameMetrics::new_now(1);
        thread::sleep(Duration::from_millis(10));
        fast.mark_present_complete();
        tracker.submit(fast);

        // Submit a slow frame (> 50ms would be if we slept longer, but let's simulate)
        let slow_start = Instant::now();
        thread::sleep(Duration::from_millis(60));
        let mut slow = FrameMetrics::new(2, slow_start);
        slow.mark_present_complete();
        tracker.submit(slow);

        assert_eq!(tracker.frames_tracked(), 2);
        // First frame should meet target, second should not
        let hit_rate = tracker.target_hit_rate();
        assert!(hit_rate < 100.0); // At least one missed
    }

    #[test]
    fn test_tracker_recent_metrics() {
        let tracker = LatencyTracker::new();

        for i in 0..5 {
            let metrics = FrameMetrics::new_now(i);
            tracker.submit(metrics);
        }

        let recent = tracker.recent_metrics(3);
        assert_eq!(recent.len(), 3);
        // Should be in reverse order (most recent first)
        assert_eq!(recent[0].sequence, 4);
        assert_eq!(recent[1].sequence, 3);
        assert_eq!(recent[2].sequence, 2);
    }

    #[test]
    fn test_tracker_summary_report() {
        let tracker = LatencyTracker::new();

        for i in 0..100 {
            let mut metrics = FrameMetrics::new_now(i);
            thread::sleep(Duration::from_micros(500)); // 0.5ms per frame
            metrics.mark_present_complete();
            tracker.submit(metrics);
        }

        let report = tracker.summary_report();
        assert_eq!(report.frames_tracked, 100);
        // Use a lenient threshold (90%) since sleep() can be delayed under load
        assert!(report.target_hit_rate > 90.0,
            "Expected >90% hit rate, got {}%", report.target_hit_rate);
    }

    #[test]
    fn test_tracker_reset() {
        let tracker = LatencyTracker::new();

        let metrics = FrameMetrics::new_now(1);
        tracker.submit(metrics);

        assert_eq!(tracker.frames_tracked(), 1);

        tracker.reset();

        assert_eq!(tracker.frames_tracked(), 0);
        assert_eq!(tracker.histogram().count(), 0);
    }

    #[test]
    fn test_shared_tracker() {
        let tracker = shared_latency_tracker();

        let tracker_clone = Arc::clone(&tracker);
        let handle = thread::spawn(move || {
            let metrics = FrameMetrics::new_now(1);
            tracker_clone.submit(metrics);
        });

        handle.join().unwrap();

        assert_eq!(tracker.frames_tracked(), 1);
    }

    #[test]
    fn test_meets_target() {
        let mut metrics = FrameMetrics::new_now(1);
        // No processing time, should be nearly instant
        assert!(metrics.meets_target(100.0));

        thread::sleep(Duration::from_millis(150));
        metrics.mark_present_complete();
        assert!(!metrics.meets_target(100.0));
    }

    #[test]
    fn test_summary_report_format() {
        let tracker = LatencyTracker::new();

        // Add some varied samples
        for latency_ms in &[10, 20, 30, 50, 80, 120] {
            let start = Instant::now();
            thread::sleep(Duration::from_millis(*latency_ms));
            let mut metrics = FrameMetrics::new(0, start);
            metrics.mark_present_complete();
            tracker.submit(metrics);
        }

        let report = tracker.summary_report();
        let formatted = report.to_string_report();

        assert!(formatted.contains("Latency Summary Report"));
        assert!(formatted.contains("Frames tracked:"));
        assert!(formatted.contains("p50:"));
        assert!(formatted.contains("p95:"));
    }
}
