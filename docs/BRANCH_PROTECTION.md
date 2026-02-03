# Branch Protection Configuration

This guide documents how to configure GitHub branch protection rules to enforce
the CI-first workflow. Branch protection is server-enforced by GitHub and provides
the strongest guarantee that broken code cannot be merged.

## Quick Setup

For automated setup, run:
```bash
./scripts/setup-branch-protection.sh
```

For manual configuration, follow the steps below.

## Manual Configuration (GitHub Web UI)

### Navigate to Settings

1. Go to the repository on GitHub
2. Click **Settings** tab
3. Click **Branches** in the left sidebar
4. Under "Branch protection rules", click **Add rule**
5. Enter `main` as the branch name pattern

### Required Settings

Enable these settings (they are critical for CI-first enforcement):

- [x] **Require a pull request before merging**
  - [x] Require approvals: `1` (adjust based on team size)
  - [x] Dismiss stale pull request approvals when new commits are pushed
  - [x] Require review from Code Owners (if using CODEOWNERS file)

- [x] **Require status checks to pass before merging**
  - [x] Require branches to be up to date before merging
  - **Required status checks:**
    - `CI Success` (the gateway job that requires all other jobs)

- [x] **Require conversation resolution before merging**

- [x] **Do not allow bypassing the above settings**
  - This prevents even repository admins from bypassing rules

### Optional Settings

Consider enabling based on team preferences:

- [ ] **Require signed commits**
  - Enforces GPG-signed commits for audit trail
  - Requires all contributors to set up GPG keys

- [ ] **Require linear history**
  - Enforces squash or rebase merges (no merge commits)
  - Creates cleaner commit history

- [ ] **Include administrators**
  - Prevents admin accounts from bypassing rules
  - Recommended for production repositories

- [ ] **Restrict who can push to matching branches**
  - Only allow specific users/teams to push
  - Useful for release management

### Save the Rule

Click **Create** or **Save changes** to apply the rule.

## Verification

After configuring branch protection:

### 1. Verify Direct Push is Blocked
```bash
git checkout main
echo "// test" >> src/lib.rs
git commit -am "test direct push"
git push
# Should fail with "protected branch" error
```

### 2. Verify PR with Failing CI Cannot Merge
```bash
git checkout -b test/protection
echo "fn broken() { " >> src/lib.rs  # Syntax error
git commit -am "test"
git push -u origin test/protection
gh pr create --fill
gh pr merge --squash
# Should fail with "required status check" error
```

### 3. Verify PR with Passing CI Can Merge
```bash
# Fix the syntax error
git checkout test/protection
# ... fix the code ...
git commit -am "fix"
git push
# Wait for CI to pass
gh pr merge --squash --delete-branch
# Should succeed
```

## How This Affects Agents

With branch protection configured, agents:

1. **Cannot push directly to main** - All changes must go through PRs
2. **Cannot merge PRs with failing CI** - The `CI Success` status check blocks merge
3. **Cannot bypass hooks** - Even `--no-verify` won't help; GitHub enforces server-side
4. **Must use the CI workflow** - There is no shortcut around these protections

The `ci merge` tool will report a clear error message if CI is not green.

## Troubleshooting

### "Required status check 'CI Success' is pending"

CI is still running. Wait for it to complete:
```bash
ci wait
```

### "Required status check 'CI Success' is failing"

CI has failed. View logs and fix:
```bash
ci logs
# ... fix issues ...
git push
ci wait
```

### "Approvals are required before merging"

If your team requires approvals, request a review:
```bash
gh pr edit --reviewer @teammate
```

### Admin needs to bypass (emergency only)

Admins can temporarily disable protection in Settings > Branches, make emergency
changes, then re-enable. This should be rare and documented.

## Related Documentation

- `AGENTS.md` - CI-first workflow for agents
- `docs/TESTING.md` - Testing infrastructure
- `scripts/setup-branch-protection.sh` - Automated setup script
