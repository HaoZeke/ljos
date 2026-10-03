//! One-use consent for an ask verdict when the caller has no approval UI.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{hook_output_ruled, HookCall, Rule};

const TTL_SECONDS: u64 = 15 * 60;
const MAX_REQUESTS: usize = 256;
const MAX_STORE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Scope {
    session: String,
    shape: String,
    tool: String,
    cwd: PathBuf,
    command: String,
    pattern: String,
    reason: String,
}

impl Scope {
    fn from_hook(input: &str, call: &HookCall, rule: &Rule) -> Result<Self> {
        let value: Value = serde_json::from_str(input)?;
        let session = call
            .session
            .as_deref()
            .filter(|s| !s.is_empty())
            .context("approval needs a conversation id")?;
        let tool = value["tool_name"]
            .as_str()
            .or_else(|| value["toolName"].as_str())
            .filter(|s| !s.is_empty())
            .context("approval needs a tool name")?;
        let base = value["cwd"]
            .as_str()
            .context("approval needs the working directory")?;
        if !Path::new(base).is_absolute() {
            bail!("approval needs an absolute working directory");
        }
        let args = value
            .get("tool_input")
            .filter(|v| !v.is_null())
            .or_else(|| value.get("toolInput"));
        let directory = args
            .and_then(|args| args.get("workdir").or_else(|| args.get("cwd")))
            .filter(|v| !v.is_null());
        let cwd = match directory {
            Some(v) => Path::new(base).join(v.as_str().context("invalid tool working directory")?),
            None => PathBuf::from(base),
        };
        let cwd = fs::canonicalize(cwd).context("approval working directory is unavailable")?;
        if !cwd.is_dir() || call.cue.is_empty() {
            bail!("approval needs a directory and a command");
        }
        Ok(Self {
            session: session.into(),
            shape: format!("{:?}", call.shape),
            tool: tool.into(),
            cwd,
            command: call.cue.clone(),
            pattern: rule.pattern.clone(),
            reason: rule.reason.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Request {
    id: String,
    scope: Scope,
    created: u64,
    approved: bool,
}

impl Request {
    fn fresh(&self, now: u64) -> bool {
        now.checked_sub(self.created)
            .is_some_and(|age| age < TTL_SECONDS)
    }
}

struct Store {
    file: File,
    requests: Vec<Request>,
}

impl Drop for Store {
    fn drop(&mut self) {
        // A child can inherit this file description until exec. Unlock it
        // explicitly so that child cannot retain the completed transaction.
        // SAFETY: the descriptor belongs to this store and is still open.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl Store {
    fn open(root: &Path, now: u64) -> Result<Self> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        let dir = fs::symlink_metadata(root)?;
        // SAFETY: geteuid has no arguments and only reads the process identity.
        let uid = unsafe { libc::geteuid() };
        if !dir.is_dir() || dir.uid() != uid || dir.mode() & 0o077 != 0 {
            bail!("approval directory must be owned by this user with mode 0700");
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root.join("requests.json"))?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            bail!("approval store must be owned by this user with mode 0600");
        }
        // The descriptor owns the lock until close. A busy store refuses the
        // request so the hook can answer within its caller's deadline.
        // SAFETY: the descriptor belongs to the open file and stays live here.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            bail!(
                "approval store is busy: {}",
                std::io::Error::last_os_error()
            );
        }
        if file.metadata()?.len() > MAX_STORE_BYTES {
            bail!("approval store exceeds its size limit");
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let mut requests: Vec<Request> = if bytes.is_empty() {
            Vec::new()
        } else {
            serde_json::from_slice(&bytes).context("invalid approval store")?
        };
        requests.retain(|request| request.fresh(now));
        Ok(Self { file, requests })
    }

    fn save(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.requests)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            bail!("approval store exceeds its size limit");
        }
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&bytes)?;
        self.file.set_len(bytes.len() as u64)?;
        self.file.sync_data()?;
        Ok(())
    }

    fn request_or_consume(&mut self, scope: Scope, now: u64) -> Result<Option<String>> {
        if let Some(index) = self
            .requests
            .iter()
            .position(|request| request.scope == scope)
        {
            if self.requests[index].approved {
                self.requests.remove(index);
                self.save()?;
                return Ok(None);
            }
            return Ok(Some(self.requests[index].id.clone()));
        }
        if self.requests.len() >= MAX_REQUESTS {
            bail!("too many pending approvals; expired requests clear after fifteen minutes");
        }
        let mut bytes = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        if self.requests.iter().any(|request| request.id == id) {
            bail!("approval id collision; retry the command");
        }
        self.requests.push(Request {
            id: id.clone(),
            scope,
            created: now,
            approved: false,
        });
        self.save()?;
        Ok(Some(id))
    }

    fn pending(&self, id: &str, session: &str) -> Result<Request> {
        let request = self
            .requests
            .iter()
            .find(|request| request.id == id)
            .context(
                "approval request is unknown, expired, or consumed; retry the original command",
            )?;
        if request.scope.session != session {
            bail!("approval belongs to a different conversation");
        }
        if request.approved {
            bail!("request is already approved; retry the original command");
        }
        Ok(request.clone())
    }

    fn confirm(&mut self, pending: &Request) -> Result<String> {
        let current = self.pending(&pending.id, &pending.scope.session)?;
        if current.scope != pending.scope || current.created != pending.created {
            bail!("approval request changed while awaiting consent");
        }
        self.approve(&pending.id)
    }

    fn approve(&mut self, id: &str) -> Result<String> {
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("approval id must be the 32 hexadecimal characters printed by the hook");
        }
        let request = self
            .requests
            .iter_mut()
            .find(|request| request.id == id)
            .context(
                "approval request is unknown, expired, or consumed; retry the original command",
            )?;
        if request.approved {
            bail!("request is already approved; retry the original command");
        }
        request.approved = true;
        let answer = format!(
            "Approved once: {}\nDirectory: {}\nConversation: {}\nRetry the original command.\n",
            request.scope.command,
            request.scope.cwd.display(),
            request.scope.session
        );
        self.save()?;
        Ok(answer)
    }
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn root() -> PathBuf {
    crate::runtime_dir().join("approvals")
}

/// Record explicit consent for the pending request named by the hook.
/// The grant permits one matching attempt within the request's lifetime.
pub fn approve(id: &str) -> Result<String> {
    // SAFETY: isatty reads one descriptor's mode and cannot fail.
    let terminal = unsafe { libc::isatty(0) } == 1;
    if crate::under_a_runner() || !terminal {
        bail!(
            "approve: consent is the person's. This runs under an agent runner or without a \
             terminal; the person runs `ljos approve {id}` in a terminal of their own"
        );
    }
    Store::open(&root(), now()?)?.approve(id)
}

/// Ask the connected client's user to consent to an existing request.
/// Tool arguments identify the request; only the client's form response grants it.
pub async fn request_approval(id: &str, peer: &rmcp::Peer<rmcp::RoleServer>) -> Result<String> {
    request_approval_at(id, &crate::holder_name(), peer, &root()).await
}

async fn request_approval_at(
    id: &str,
    session: &str,
    peer: &rmcp::Peer<rmcp::RoleServer>,
    root: &Path,
) -> Result<String> {
    use rmcp::model::{ElicitRequestParams, ElicitationSchema};

    let started = now()?;
    let pending = Store::open(root, started)?.pending(id, session)?;
    let info = peer
        .peer_info()
        .context("approval client has not initialized")?;
    let supports_form = info.capabilities.elicitation.as_ref().is_some_and(|cap| {
        // The original elicitation capability is an empty object and means form.
        cap.form.is_some() || cap.url.is_none()
    });
    if !supports_form {
        bail!("this client cannot display a consent form; run `ljos approve {id}` in your own terminal");
    }
    let schema = ElicitationSchema::from_json_schema(
        serde_json::json!({
            "type": "object",
            "properties": {
                "approve": {
                    "type": "boolean",
                    "title": "Allow this command once",
                    "description": "Consent to the exact command and directory shown above.",
                    "default": false
                }
            },
            "required": ["approve"]
        })
        .as_object()
        .expect("object schema")
        .clone(),
    )?;
    let message = format!(
        "Allow one attempt of this command?\nCommand (JSON): {}\nDirectory: {}\nConversation: {}\nRule: {}\nReason: {}\nRequest: {}\nThe grant expires fifteen minutes after the request was created.",
        serde_json::to_string(&pending.scope.command)?,
        serde_json::to_string(&pending.scope.cwd)?,
        pending.scope.session, pending.scope.pattern, pending.scope.reason, pending.id
    );
    // No store lock spans the user interaction. Confirmation reopens it and
    // checks expiry, identity and contents before granting the one retry.
    let remaining = TTL_SECONDS.saturating_sub(started.saturating_sub(pending.created));
    let answer = peer
        .create_elicitation_with_timeout(
            ElicitRequestParams::FormElicitationParams {
                meta: None,
                message,
                requested_schema: schema,
            },
            Some(std::time::Duration::from_secs(remaining)),
        )
        .await
        .context("client confirmation failed; command remains blocked")?;
    if !consents(&answer) {
        bail!("consent was not granted; command remains blocked");
    }
    Store::open(root, now()?)?.confirm(&pending)
}

fn consents(answer: &rmcp::model::ElicitResult) -> bool {
    answer.action == rmcp::model::ElicitationAction::Accept
        && answer.content.as_ref().and_then(|v| v.get("approve")) == Some(&Value::Bool(true))
}

/// Apply a one-use grant only to an ask verdict. Denials reach the caller
/// unchanged, including denials from the policy binary.
pub fn hook_output(input: &str, call: &HookCall, context: &str, verdict: Option<&Rule>) -> String {
    hook_output_at(input, call, context, verdict, &root(), now())
}

fn hook_output_at(
    input: &str,
    call: &HookCall,
    context: &str,
    verdict: Option<&Rule>,
    root: &Path,
    now: Result<u64>,
) -> String {
    let Some(rule) = verdict.filter(|rule| rule.verdict == "ask") else {
        return hook_output_ruled(call, context, verdict);
    };
    if call.event != "PreToolUse" || call.shape.asks() {
        return hook_output_ruled(call, context, verdict);
    }
    let result = (|| {
        let scope = Scope::from_hook(input, call, rule)?;
        let now = now?;
        Store::open(root, now)?.request_or_consume(scope, now)
    })();
    match result {
        Ok(None) => hook_output_ruled(call, context, None),
        result => {
            let mut pending = rule.clone();
            let detail = match result {
                Ok(Some(id)) => format!("Call the ljos_request_approval MCP tool with id {id} to show the person a consent form. If that client cannot display the form, ask the person to run `ljos approve {id}` in a terminal of their own (it refuses under an agent). Retry only after consent is recorded. The grant is for one attempt in this directory and conversation and expires fifteen minutes after the request."),
                Err(error) => format!("Approval could not be recorded: {error:#}. The command remains blocked."),
                Ok(None) => unreachable!(),
            };
            pending.reason = format!("{} {detail}", rule.reason);
            hook_output_ruled(call, context, Some(&pending))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_call;

    fn ask() -> Rule {
        Rule {
            pattern: "git push*".into(),
            verdict: "ask".into(),
            reason: "A push needs consent.".into(),
        }
    }

    fn input(cwd: &Path) -> String {
        serde_json::json!({"hook_event_name":"PreToolUse", "turn_id":"turn-1",
            "session_id":"conversation-1", "cwd":cwd, "tool_name":"Bash",
            "tool_input":{"command":"git push origin main"}})
        .to_string()
    }

    fn output(input: &str, rule: &Rule, root: &Path, now: u64) -> String {
        hook_output_at(input, &hook_call(input), "", Some(rule), root, Ok(now))
    }

    fn request_id(output: &str) -> String {
        let value: Value = serde_json::from_str(output).unwrap();
        assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        reason
            .split("ljos approve ")
            .nth(1)
            .unwrap()
            .split('`')
            .next()
            .unwrap()
            .into()
    }

    #[test]
    fn approval_allows_one_matching_retry_and_cannot_be_replayed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        assert_eq!(request_id(&output(&input, &ask(), &root, 101)), id);
        Store::open(&root, 102).unwrap().approve(&id).unwrap();
        assert!(output(&input, &ask(), &root, 103).is_empty());
        assert!(Store::open(&root, 104).unwrap().approve(&id).is_err());
        let other = request_id(&output(&input, &ask(), &root, 105));
        assert_ne!(id, other);
        assert!(Store::open(&root, 106).unwrap().approve(&id).is_err());
    }

    #[test]
    fn grants_are_bound_to_command_directory_conversation_tool_and_rule() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let other = tempfile::tempdir().unwrap();
        let original = input(temp.path());
        let id = request_id(&output(&original, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        for (pointer, replacement) in [
            ("/tool_input/command", "git push fork main"),
            ("/cwd", other.path().to_str().unwrap()),
            ("/session_id", "conversation-2"),
            ("/tool_name", "terminal"),
        ] {
            let mut changed: Value = serde_json::from_str(&original).unwrap();
            *changed.pointer_mut(pointer).unwrap() = replacement.into();
            assert_ne!(
                request_id(&output(&changed.to_string(), &ask(), &root, 102)),
                id
            );
        }
        for rule in [
            Rule {
                pattern: "git *".into(),
                ..ask()
            },
            Rule {
                reason: "A different approval requirement.".into(),
                ..ask()
            },
        ] {
            assert_ne!(request_id(&output(&original, &rule, &root, 103)), id);
        }
        assert!(output(&original, &ask(), &root, 104).is_empty());
    }

    #[test]
    fn a_tool_workdir_is_part_of_the_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let other = tempfile::tempdir().unwrap();
        let original = input(temp.path());
        let id = request_id(&output(&original, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        let mut changed: Value = serde_json::from_str(&original).unwrap();
        changed["tool_input"]["workdir"] = serde_json::json!(other.path());
        assert_ne!(
            request_id(&output(&changed.to_string(), &ask(), &root, 102)),
            id
        );
        assert!(output(&original, &ask(), &root, 103).is_empty());
    }

    #[test]
    fn expired_and_future_dated_requests_do_not_authorize() {
        for approval_time in [99, 100 + TTL_SECONDS] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("approvals");
            let input = input(temp.path());
            let id = request_id(&output(&input, &ask(), &root, 100));
            assert!(Store::open(&root, approval_time)
                .unwrap()
                .approve(&id)
                .is_err());
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        assert_ne!(
            request_id(&output(&input, &ask(), &root, 100 + TTL_SECONDS)),
            id
        );
    }

    #[test]
    fn deny_verdicts_win_over_an_approved_request() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        for pattern in ["ljos-policyd", "git push*"] {
            let deny = Rule {
                pattern: pattern.into(),
                verdict: "deny".into(),
                reason: "Denied.".into(),
            };
            assert_eq!(
                output(&input, &deny, &root, 102),
                hook_output_ruled(&hook_call(&input), "", Some(&deny))
            );
        }
        assert!(output(&input, &ask(), &root, 103).is_empty());
    }

    #[test]
    fn a_native_ask_does_not_create_a_local_grant() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let mut value: Value = serde_json::from_str(&input(temp.path())).unwrap();
        value.as_object_mut().unwrap().remove("turn_id");
        let input = value.to_string();
        let response: Value = serde_json::from_str(&output(&input, &ask(), &root, 100)).unwrap();
        assert_eq!(response["hookSpecificOutput"]["permissionDecision"], "ask");
        assert!(!root.exists());
    }

    #[test]
    fn approval_does_not_depend_on_the_retry_turn_id() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        let mut retry: Value = serde_json::from_str(&input).unwrap();
        retry["turn_id"] = "turn-2".into();
        assert!(output(&retry.to_string(), &ask(), &root, 102).is_empty());
    }

    #[test]
    fn missing_scope_and_corrupt_state_keep_the_command_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let original = input(temp.path());
        for key in ["session_id", "cwd", "tool_name"] {
            let mut value: Value = serde_json::from_str(&original).unwrap();
            value.as_object_mut().unwrap().remove(key);
            let response: Value =
                serde_json::from_str(&output(&value.to_string(), &ask(), &root, 100)).unwrap();
            assert_eq!(response["hookSpecificOutput"]["permissionDecision"], "deny");
            assert!(!root.exists());
        }
        request_id(&output(&original, &ask(), &root, 101));
        fs::write(root.join("requests.json"), "not json").unwrap();
        let response: Value = serde_json::from_str(&output(&original, &ask(), &root, 102)).unwrap();
        assert_eq!(response["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    fn a_duplicate_descriptor_does_not_retain_a_finished_transaction() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let store = Store::open(&root, 100).unwrap();
        let inherited = store.file.try_clone().unwrap();
        drop(store);
        let next = Store::open(&root, 101).unwrap();
        assert!(next.requests.is_empty());
        drop(inherited);
    }

    #[test]
    fn simultaneous_retries_consume_at_most_one_grant() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        Store::open(&root, 101).unwrap().approve(&id).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                let input = input.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    output(&input, &ask(), &root, 102).is_empty()
                })
            })
            .collect();
        let allowed = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|allowed| *allowed)
            .count();
        assert_eq!(allowed, 1);
    }
    #[test]
    fn client_consent_requires_accept_and_a_true_boolean() {
        use rmcp::model::{ElicitResult, ElicitationAction};
        for action in [
            ElicitationAction::Accept,
            ElicitationAction::Decline,
            ElicitationAction::Cancel,
        ] {
            for content in [
                None,
                Some(serde_json::json!({})),
                Some(serde_json::json!({"approve":false})),
                Some(serde_json::json!({"approve":"true"})),
                Some(serde_json::json!({"approve":1})),
                Some(serde_json::json!({"approve":true})),
            ] {
                let mut answer = ElicitResult::new(action.clone());
                answer.content = content.clone();
                assert_eq!(
                    consents(&answer),
                    action == ElicitationAction::Accept
                        && content == Some(serde_json::json!({"approve":true}))
                );
            }
        }
    }

    #[test]
    fn a_confirmation_rechecks_conversation_contents_expiry_and_single_use() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("approvals");
        let input = input(temp.path());
        let id = request_id(&output(&input, &ask(), &root, 100));
        let pending = {
            let store = Store::open(&root, 101).unwrap();
            assert!(store.pending(&id, "conversation-2").is_err());
            store.pending(&id, "conversation-1").unwrap()
        };
        // A dialog holds no lock, so another hook can inspect the same request.
        assert_eq!(request_id(&output(&input, &ask(), &root, 102)), id);
        for changed in ["command", "directory", "rule", "created"] {
            let mut snapshot = pending.clone();
            match changed {
                "command" => snapshot.scope.command.push_str(" --force"),
                "directory" => snapshot.scope.cwd = temp.path().join("elsewhere"),
                "rule" => snapshot.scope.reason.push_str(" changed"),
                "created" => snapshot.created += 1,
                _ => unreachable!(),
            }
            assert!(Store::open(&root, 103).unwrap().confirm(&snapshot).is_err());
        }
        Store::open(&root, 104).unwrap().confirm(&pending).unwrap();
        assert!(Store::open(&root, 105).unwrap().confirm(&pending).is_err());
        assert!(output(&input, &ask(), &root, 106).is_empty());
        assert!(Store::open(&root, 107).unwrap().confirm(&pending).is_err());

        let id = request_id(&output(&input, &ask(), &root, 200));
        let pending = Store::open(&root, 201)
            .unwrap()
            .pending(&id, "conversation-1")
            .unwrap();
        assert!(Store::open(&root, 200 + TTL_SECONDS)
            .unwrap()
            .confirm(&pending)
            .is_err());
        assert_ne!(
            request_id(&output(&input, &ask(), &root, 200 + TTL_SECONDS)),
            id
        );
    }
}
