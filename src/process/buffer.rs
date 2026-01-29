//! Frame buffer management and pooling
//!
//! Provides efficient buffer management to avoid per-frame allocation
//! and support for zero-copy paths where possible.
#![allow(dead_code)] // Buffer pool infrastructure
//!
//! # Design
//!
//! - Fixed-size pool allocated at startup
//! - Reference-counted buffers for automatic release
//! - No blocking on pool exhaustion (drop frames instead)
//! - Configurable pool size and frame dimensions

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// Configuration for a frame buffer pool
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of buffers in the pool
    pub capacity: usize,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Bytes per pixel (typically 4 for RGBA)
    pub bytes_per_pixel: usize,
    /// Warn if buffer held longer than this duration
    pub hold_warning_threshold: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            capacity: 4,
            width: 1920,
            height: 1080,
            bytes_per_pixel: 4,
            hold_warning_threshold: Duration::from_millis(100),
        }
    }
}

impl PoolConfig {
    /// Create config for a specific resolution
    pub fn for_resolution(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..Default::default()
        }
    }

    /// Calculate buffer size in bytes
    ///
    /// Uses checked arithmetic to prevent overflow on 32-bit systems.
    /// Returns None if the calculation would overflow.
    pub fn buffer_size_checked(&self) -> Option<usize> {
        let pixels = (self.width as usize).checked_mul(self.height as usize)?;
        pixels.checked_mul(self.bytes_per_pixel)
    }

    /// Calculate buffer size in bytes
    ///
    /// # Panics
    /// Panics if the calculation would overflow (extremely large dimensions).
    /// For safe handling, use `buffer_size_checked()` instead.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size_checked()
            .expect("Buffer size calculation overflow - dimensions too large")
    }

    /// Calculate total pool memory usage in bytes
    ///
    /// Uses checked arithmetic to prevent overflow.
    /// Returns None if the calculation would overflow.
    pub fn total_memory_checked(&self) -> Option<usize> {
        let buffer_size = self.buffer_size_checked()?;
        buffer_size.checked_mul(self.capacity)
    }

    /// Calculate total pool memory usage in bytes
    ///
    /// # Panics
    /// Panics if the calculation would overflow.
    /// For safe handling, use `total_memory_checked()` instead.
    pub fn total_memory(&self) -> usize {
        self.total_memory_checked()
            .expect("Total memory calculation overflow - pool too large")
    }
}

/// Statistics tracked by the buffer pool
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Total acquisitions
    pub acquisitions: AtomicU64,
    /// Acquisition failures (pool exhausted)
    pub failures: AtomicU64,
    /// Buffers currently in use
    pub in_use: AtomicU64,
    /// Maximum buffers ever in use simultaneously
    pub peak_in_use: AtomicU64,
}

impl PoolStats {
    fn record_acquire(&self) {
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        let current = self.in_use.fetch_add(1, Ordering::Relaxed) + 1;
        
        // Update peak if needed
        let mut peak = self.peak_in_use.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_in_use.compare_exchange_weak(
                peak, current, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }

    fn record_release(&self) {
        self.in_use.fetch_sub(1, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current snapshot
    pub fn snapshot(&self) -> PoolStatsSnapshot {
        PoolStatsSnapshot {
            acquisitions: self.acquisitions.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            in_use: self.in_use.load(Ordering::Relaxed),
            peak_in_use: self.peak_in_use.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStatsSnapshot {
    pub acquisitions: u64,
    pub failures: u64,
    pub in_use: u64,
    pub peak_in_use: u64,
}

impl PoolStatsSnapshot {
    /// Calculate failure rate as percentage
    pub fn failure_rate(&self) -> f64 {
        let total = self.acquisitions + self.failures;
        if total == 0 {
            0.0
        } else {
            (self.failures as f64 / total as f64) * 100.0
        }
    }
}

/// Internal state for a buffer slot
struct BufferSlot {
    /// The actual data buffer
    /// Using UnsafeCell to allow interior mutability when slot is acquired exclusively
    data: std::cell::UnsafeCell<Vec<u8>>,
    /// Whether this slot is currently in use
    in_use: AtomicBool,
    /// When this buffer was acquired (for timeout warnings)
    acquired_at: Mutex<Option<Instant>>,
}

// SAFETY: BufferSlot is Send because:
// - data is only accessed mutably when in_use is true (exclusive access)
// - in_use provides synchronization via atomic operations
// - acquired_at is protected by Mutex
unsafe impl Send for BufferSlot {}

// SAFETY: BufferSlot is Sync because:
// - data access is gated by in_use atomic flag (only one accessor at a time)
// - in_use uses Acquire/Release ordering for proper synchronization
// - acquired_at uses Mutex for thread-safe access
unsafe impl Sync for BufferSlot {}

impl BufferSlot {
    fn new(size: usize) -> Self {
        Self {
            data: std::cell::UnsafeCell::new(vec![0u8; size]),
            in_use: AtomicBool::new(false),
            acquired_at: Mutex::new(None),
        }
    }

    /// Get immutable access to data
    /// SAFETY: Caller must ensure exclusive access (in_use flag set)
    unsafe fn data(&self) -> &[u8] {
        &*self.data.get()
    }

    /// Get mutable access to data
    /// SAFETY: Caller must ensure exclusive access (in_use flag set)
    unsafe fn data_mut(&self) -> &mut [u8] {
        &mut *self.data.get()
    }

    /// Get data length without requiring mutable access
    fn data_len(&self) -> usize {
        // SAFETY: Reading length is safe as Vec layout is stable
        unsafe { (*self.data.get()).len() }
    }

    fn try_acquire(&self) -> bool {
        if self.in_use.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            *self.acquired_at.lock().unwrap() = Some(Instant::now());
            true
        } else {
            false
        }
    }

    fn release(&self) {
        *self.acquired_at.lock().unwrap() = None;
        self.in_use.store(false, Ordering::Release);
    }

    fn hold_duration(&self) -> Option<Duration> {
        self.acquired_at.lock().unwrap().map(|t| t.elapsed())
    }
}

/// A frame buffer pool for efficient memory management
///
/// Buffers are allocated at pool creation and reused. When a buffer is
/// acquired, it is wrapped in a `FrameBuffer` which automatically returns
/// it to the pool when dropped.
pub struct FrameBufferPool {
    config: PoolConfig,
    slots: Vec<Arc<BufferSlot>>,
    stats: Arc<PoolStats>,
    /// Self-reference for buffer return (reserved for future auto-reclamation)
    #[allow(dead_code)]
    pool_ref: Weak<FrameBufferPool>,
}

impl FrameBufferPool {
    /// Create a new buffer pool with the given configuration
    ///
    /// # Panics
    /// Panics if the buffer dimensions would cause integer overflow.
    /// For fallible creation, use `try_new()` instead.
    pub fn new(config: PoolConfig) -> Arc<Self> {
        Self::try_new(config).expect("Failed to create buffer pool: dimensions too large")
    }

    /// Try to create a new buffer pool with the given configuration
    ///
    /// Returns `None` if the buffer dimensions would cause integer overflow.
    pub fn try_new(config: PoolConfig) -> Option<Arc<Self>> {
        // Validate dimensions don't overflow
        let buffer_size = config.buffer_size_checked()?;

        let slots: Vec<_> = (0..config.capacity)
            .map(|_| Arc::new(BufferSlot::new(buffer_size)))
            .collect();

        Some(Arc::new_cyclic(|weak| Self {
            config,
            slots,
            stats: Arc::new(PoolStats::default()),
            pool_ref: weak.clone(),
        }))
    }

    /// Create a pool with default settings for the given resolution
    pub fn for_resolution(width: u32, height: u32) -> Arc<Self> {
        Self::new(PoolConfig::for_resolution(width, height))
    }

    /// Try to acquire a buffer from the pool
    ///
    /// Returns `None` if all buffers are in use.
    /// Callers should handle this by dropping a frame rather than blocking.
    pub fn try_acquire(self: &Arc<Self>) -> Option<FrameBuffer> {
        for (idx, slot) in self.slots.iter().enumerate() {
            if slot.try_acquire() {
                self.stats.record_acquire();
                return Some(FrameBuffer {
                    pool: Arc::clone(self),
                    slot_idx: idx,
                });
            }
        }

        self.stats.record_failure();
        None
    }

    /// Get pool configuration
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Get the number of available (not in use) buffers
    pub fn available(&self) -> usize {
        self.slots.iter().filter(|s| !s.in_use.load(Ordering::Relaxed)).count()
    }

    /// Check for buffers held too long and log warnings
    pub fn check_held_buffers(&self) {
        let threshold = self.config.hold_warning_threshold;
        for (idx, slot) in self.slots.iter().enumerate() {
            if let Some(duration) = slot.hold_duration() {
                if duration > threshold {
                    tracing::warn!(
                        buffer_idx = idx,
                        held_ms = duration.as_millis() as u64,
                        threshold_ms = threshold.as_millis() as u64,
                        "Buffer held longer than threshold"
                    );
                }
            }
        }
    }

    fn release_slot(&self, idx: usize) {
        if idx < self.slots.len() {
            self.slots[idx].release();
            self.stats.record_release();
        }
    }
}

/// A buffer acquired from a pool
///
/// Provides mutable access to the underlying data and automatically
/// returns the buffer to the pool when dropped.
pub struct FrameBuffer {
    pool: Arc<FrameBufferPool>,
    slot_idx: usize,
}

impl FrameBuffer {
    /// Get the buffer dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.pool.config.width, self.pool.config.height)
    }

    /// Get the buffer data as a slice
    pub fn data(&self) -> &[u8] {
        let slot = &self.pool.slots[self.slot_idx];
        // SAFETY: We have exclusive access via FrameBuffer ownership
        // and the slot is marked in_use (checked in try_acquire)
        unsafe { slot.data() }
    }

    /// Get the buffer data as a mutable slice
    pub fn data_mut(&mut self) -> &mut [u8] {
        let slot = &self.pool.slots[self.slot_idx];
        // SAFETY: We have exclusive mutable access via &mut self
        // and the slot is marked in_use (checked in try_acquire)
        unsafe { slot.data_mut() }
    }

    /// Get buffer size in bytes
    pub fn size(&self) -> usize {
        self.pool.slots[self.slot_idx].data_len()
    }

    /// Copy data into this buffer
    pub fn copy_from(&mut self, src: &[u8]) {
        let data = self.data_mut();
        let len = src.len().min(data.len());
        data[..len].copy_from_slice(&src[..len]);
    }
}

impl Drop for FrameBuffer {
    fn drop(&mut self) {
        self.pool.release_slot(self.slot_idx);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert_eq!(config.capacity, 4);
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.bytes_per_pixel, 4);
    }

    #[test]
    fn test_buffer_size_calculation() {
        let config = PoolConfig::for_resolution(1920, 1080);
        assert_eq!(config.buffer_size(), 1920 * 1080 * 4);

        let config_4k = PoolConfig::for_resolution(3840, 2160);
        assert_eq!(config_4k.buffer_size(), 3840 * 2160 * 4);
    }

    #[test]
    fn test_total_memory_calculation() {
        let config = PoolConfig {
            capacity: 4,
            width: 1920,
            height: 1080,
            bytes_per_pixel: 4,
            ..Default::default()
        };
        assert_eq!(config.total_memory(), 1920 * 1080 * 4 * 4);
    }

    #[test]
    fn test_pool_creation() {
        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 3,
            width: 640,
            height: 480,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        assert_eq!(pool.available(), 3);
        assert_eq!(pool.config().capacity, 3);
    }

    #[test]
    fn test_acquire_and_release() {
        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 2,
            width: 4,
            height: 4,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        assert_eq!(pool.available(), 2);

        let buf1 = pool.try_acquire();
        assert!(buf1.is_some());
        assert_eq!(pool.available(), 1);

        let buf2 = pool.try_acquire();
        assert!(buf2.is_some());
        assert_eq!(pool.available(), 0);

        // Pool exhausted
        let buf3 = pool.try_acquire();
        assert!(buf3.is_none());

        // Release one
        drop(buf1);
        assert_eq!(pool.available(), 1);

        // Can acquire again
        let buf4 = pool.try_acquire();
        assert!(buf4.is_some());
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_buffer_data_access() {
        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 1,
            width: 2,
            height: 2,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        let mut buf = pool.try_acquire().unwrap();
        assert_eq!(buf.size(), 16); // 2 * 2 * 4

        // Write to buffer
        let data = buf.data_mut();
        data[0] = 255;
        data[1] = 128;

        // Read back
        assert_eq!(buf.data()[0], 255);
        assert_eq!(buf.data()[1], 128);
    }

    #[test]
    fn test_buffer_copy_from() {
        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 1,
            width: 2,
            height: 2,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        let mut buf = pool.try_acquire().unwrap();
        let src = vec![1, 2, 3, 4, 5, 6, 7, 8];
        buf.copy_from(&src);

        assert_eq!(&buf.data()[..8], &src[..]);
    }

    #[test]
    fn test_stats_tracking() {
        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 1,
            width: 4,
            height: 4,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        // Initial stats
        let stats = pool.stats().snapshot();
        assert_eq!(stats.acquisitions, 0);
        assert_eq!(stats.failures, 0);
        assert_eq!(stats.in_use, 0);

        // Acquire
        let buf = pool.try_acquire();
        assert!(buf.is_some());
        let stats = pool.stats().snapshot();
        assert_eq!(stats.acquisitions, 1);
        assert_eq!(stats.in_use, 1);
        assert_eq!(stats.peak_in_use, 1);

        // Try acquire when exhausted
        let buf2 = pool.try_acquire();
        assert!(buf2.is_none());
        let stats = pool.stats().snapshot();
        assert_eq!(stats.failures, 1);

        // Release
        drop(buf);
        let stats = pool.stats().snapshot();
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.peak_in_use, 1); // Peak preserved
    }

    #[test]
    fn test_failure_rate_calculation() {
        let snapshot = PoolStatsSnapshot {
            acquisitions: 90,
            failures: 10,
            in_use: 0,
            peak_in_use: 2,
        };

        assert!((snapshot.failure_rate() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_failure_rate_zero_attempts() {
        let snapshot = PoolStatsSnapshot {
            acquisitions: 0,
            failures: 0,
            in_use: 0,
            peak_in_use: 0,
        };

        assert_eq!(snapshot.failure_rate(), 0.0);
    }

    #[test]
    fn test_buffer_dimensions() {
        let pool = FrameBufferPool::for_resolution(1280, 720);
        let buf = pool.try_acquire().unwrap();

        assert_eq!(buf.dimensions(), (1280, 720));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let pool = FrameBufferPool::new(PoolConfig {
            capacity: 4,
            width: 4,
            height: 4,
            bytes_per_pixel: 4,
            ..Default::default()
        });

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let pool = Arc::clone(&pool);
                thread::spawn(move || {
                    for _ in 0..10 {
                        if let Some(mut buf) = pool.try_acquire() {
                            buf.data_mut()[0] = i as u8;
                            thread::sleep(Duration::from_micros(100));
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All buffers should be released
        assert_eq!(pool.available(), 4);
    }
}
