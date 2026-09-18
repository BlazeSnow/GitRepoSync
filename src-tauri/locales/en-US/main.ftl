# Git Repo Sync backend messages (English).
# Keys mirror locales/zh-CN/main.ftl; missing keys fall back to zh-CN.

# ---------- Common ----------
db-error = Database error: {$err}
join-sep = ;

# ---------- Auth ----------
invalid-credentials = Incorrect username or password
login-failed-log = Login failed (incorrect username or password)
session-expired = Session expired, please sign in again
new-password-too-short = New password must be at least 6 characters
user-not-found = User not found
wrong-password = Old password is incorrect
log-login = Signed in
log-logout = Signed out
log-change-password = Changed password

# ---------- Repositories ----------
repo-fields-empty = Repository name, source and target must not be empty
repo-not-found = Repository not found
repo-syncing = This repository is syncing; stop it first
log-repo-added = Added repository "{$name}"
log-repo-edited = Edited repository "{$name}"
log-repo-deleted = Deleted repository "{$name}"

# ---------- Sync ----------
log-sync-started = Started syncing repository "{$name}"
log-sync-success = Synced repository "{$name}" successfully
log-sync-failed = Failed to sync repository "{$name}"
log-sync-stopped = Sync of repository "{$name}" was stopped
log-sync-stopped-cmd = Stopped sync
step-clone = Cloned source repository to local staging directory
step-fetch = Pulled updates from source repository
step-lfs = Updated LFS files
step-submodule = Updated submodules
step-push = Pushed to target repository
lfs-skipped = git-lfs not detected; skipped LFS update
manually-stopped = Manually stopped
git-spawn-error = Failed to launch git: {$err} (make sure Git is installed and on PATH)
git-wait-error = Failed to wait for git process
process-terminated = Process was terminated

# ---------- Providers ----------

# ---------- Settings ----------
base-dir-empty = Base directory must not be empty
log-base-dir-changed = Changed repository base directory to {$dir}
log-mcp-key-regenerated = Regenerated MCP API key

# ---------- MCP ----------
mcp-unauthorized = unauthorized: incorrect API key
mcp-parse-error = parse error
mcp-method-not-found = method not found: {$method}
mcp-unknown-tool = unknown tool: {$tool}
mcp-spawn-internal = Internal error
log-mcp-repo-added = Added repository "{$name}" via MCP
log-mcp-repo-deleted = Deleted a repository via MCP
log-mcp-sync = Triggered sync via MCP
log-mcp-base-dir-changed = Changed repository base directory to {$dir} via MCP

# ---------- MCP tool descriptions ----------
tool-list-repos = List all configured sync repositories with their latest sync status
tool-add-repo = Add a sync repository: pull from the source into the local base directory as a staging copy (updating LFS and submodules), then push to the target repository
tool-add-repo-name = Repository name (also the directory name under the local base directory)
tool-add-repo-source = Source URL (Git repository URL)
tool-add-repo-target = Target repository URL (Git repository URL)
tool-remove-repo = Delete the specified sync repository
tool-remove-repo-id = Repository ID
tool-sync-repo = Start syncing the specified repository immediately (async; use get_sync_status to check progress)
tool-sync-repo-id = Repository ID
tool-get-sync-status = Query the latest sync status of all repositories
tool-get-base-dir = Query the local repository base directory (staging directory)
tool-set-base-dir = Change the local repository base directory (staging directory)
tool-set-base-dir-base-dir = Base directory path, may start with ~
