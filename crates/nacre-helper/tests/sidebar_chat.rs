//! Black-box coverage for the native sidebar socket using fixture-only providers.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const FAKE_CODEX: &str = r##"#!/usr/bin/python3
import json, os, signal, socket, sys
import time
log = os.environ['FAKE_PROVIDER_LOG']
scenario = os.environ.get('FAKE_SCENARIO', 'chat')
signal.alarm(120)
def record(row):
    with open(log, 'a', encoding='utf-8') as f:
        f.write(json.dumps(row) + '\n')
        f.flush()
def emit(row):
    sys.stdout.write(json.dumps(row) + '\n')
    sys.stdout.flush()
def read():
    line = sys.stdin.readline()
    if not line:
        return None
    row = json.loads(line)
    record({'kind': 'input', 'pid': os.getpid(), 'row': row})
    return row
def reply(ident, result):
    emit({'id': ident, 'result': result})
def notification(method, params):
    params.setdefault('threadId', 'fixture-thread')
    emit({'method': method, 'params': params})
def ask(ident, method, params):
    emit({'id': ident, 'method': method, 'params': params})
    while True:
        row = read()
        if row is None:
            raise SystemExit(0)
        if row.get('id') == ident and 'result' in row:
            record({'kind': 'rpc-answer', 'pid': os.getpid(), 'id': ident, 'result': row['result']})
            return row['result']
        if row.get('id') is not None:
            reply(row['id'], {})
turn_state = os.environ['FAKE_TURN_STATE']
try:
    turn = int(open(turn_state, encoding='utf-8').read())
except (FileNotFoundError, ValueError):
    turn = 0
provider_count_path = os.environ['FAKE_PROVIDER_COUNT']
try:
    provider_instance = int(open(provider_count_path, encoding='utf-8').read()) + 1
except (FileNotFoundError, ValueError):
    provider_instance = 1
with open(provider_count_path, 'w', encoding='utf-8') as f:
    f.write(str(provider_instance))
rpc_id = 500
record({'kind': 'pid', 'pid': os.getpid(), 'executable': os.path.realpath(sys.argv[0]), 'code_home': os.environ.get('CODEX_HOME'), 'provider_instance':provider_instance})
while True:
    row = read()
    if row is None:
        break
    method, ident, params = row.get('method'), row.get('id'), row.get('params', {})
    if method == 'initialize':
        reply(ident, {'capabilities': {}})
    elif method == 'thread/start':
        reply(ident, {'thread': {'id': 'fixture-thread', 'turns': []}})
    elif method == 'thread/resume':
        if scenario == 'question-reconnect':
            reply(ident, {'thread': {'id': params['threadId'], 'turns': []}})
            continue
        elif scenario == 'resume-race':
            def stale_peer_dropped(signum, frame):
                record({'kind':'resume-peer-dropped','pid':os.getpid()})
                socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM).sendto(b'peer-dropped', os.environ['FAKE_RESUME_READY'])
                raise SystemExit(0)
            signal.signal(signal.SIGTERM, stale_peer_dropped)
            release = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
            release.bind(os.environ['FAKE_RESUME_RELEASE'])
            release.settimeout(60)
            socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM).sendto(b'ready', os.environ['FAKE_RESUME_READY'])
            release.recvfrom(64)
            turns = [{'items': [
                {'type':'userMessage','id':'stale-user','content':[{'type':'text','text':'stale transcript'}]},
                {'type':'agentMessage','id':'stale-agent','text':'stale response'}]}]
            # The server may terminate this stale peer as soon as it reads the
            # reply. Flush the marker first; observe termination separately.
            record({'kind':'resume-response-released','pid':os.getpid()})
            reply(ident, {'thread': {'id': 'stale-thread-id', 'turns': turns}})
            continue
        elif scenario == 'async':
            turns = [
                {'items': [{'type':'agentMessage','id':'old-question','text':'old answer','questions':[{'title':'Old question','options':['Old']}]}]},
                {'items': [{'type':'userMessage','id':'latest-user','content':[{'type':'text','text':'latest input'}]}]},
                {'items': [{'type':'agentMessage','id':'new-question','text':'new answer','questions':[{'title':'New question','options':['A','B']}]}]},
            ]
        else:
            turns = [{'items': [
                {'type': 'userMessage', 'id': 'old-user', 'content': [{'type': 'text', 'text': 'earlier question'}]},
                {'type': 'agentMessage', 'id': 'old-agent', 'text': 'earlier answer'}]}]
        reply(ident, {'thread': {'id': params['threadId'], 'turns': turns}})
    elif method == 'thread/name/set':
        reply(ident, {})
    elif method == 'turn/start':
        turn += 1
        with open(turn_state, 'w', encoding='utf-8') as f:
            f.write(str(turn))
        turn_id = 'fixture-turn-' + str(turn)
        reply(ident, {'turn': {'id': turn_id}})
        notification('turn/started', {'turn': {'id': turn_id}})
        notification('item/completed', {'item': {'type': 'userMessage', 'id': 'user-' + str(turn), 'content': params.get('input', [])}})
        notification('item/agentMessage/delta', {'itemId': 'assistant-' + str(turn), 'delta': 'stream-' + str(turn)})
        if scenario == 'workspace':
            notification('item/completed', {'item': {'type':'agentMessage','id':'assistant-' + str(turn),'text':'workspace answer'}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
            continue
        if scenario == 'question-reconnect':
            question = {'questions':[{'id':'choice','question':'Retry question?','options':['A','B']}]}
            if provider_instance == 1:
                emit({'id':900,'method':'item/tool/requestUserInput','params':question})
                record({'kind':'question-sent','pid':os.getpid(),'provider_instance':provider_instance,'rpc_id':900})
                raise SystemExit(0)
            record({'kind':'question-sent','pid':os.getpid(),'provider_instance':provider_instance,'rpc_id':900})
            ask(900, 'item/tool/requestUserInput', question)
            notification('item/completed', {'item': {'type':'agentMessage','id':'retry-answer','text':'completed after retry'}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
            continue
        if scenario == 'resume-race':
            notification('item/completed', {'item': {'type':'agentMessage','id':'fresh-agent','text':'fresh chat answer'}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
            continue
        if scenario == 'async':
            notification('item/completed', {'item': {'type':'agentMessage','id':'async-answer-' + str(turn),'text':'async reply'}})
            notification('item/started', {'item': {'type':'agentMessage','id':'live-question-item','questions':[{'title':'Question after finish?','options':['Yes']}]}})
            rpc_id += 1
            ask(rpc_id, 'item/tool/requestUserInput', {'questions':[{'id':'coexist','header':'Keep both','question':'Blocking question','options':[{'label':'Yes'}]}]})
            notification('item/completed', {'item': {'type':'agentMessage','id':'live-question-item','text':'tool completed','questions':[{'title':'Question after finish?','options':['Yes']} ]}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
            continue
        if scenario in ('steer-failed','steer-race') and turn > 1:
            notification('item/completed', {'item': {'type':'agentMessage','id':'assistant-' + str(turn),'text':'fallback answer'}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
            continue
        if scenario in ('steer-failed','steer-race') and turn == 1:
            continue
        if turn == 1:
            rpc_id += 1
            ask(rpc_id, 'item/tool/requestUserInput', {'questions': [{'id': 'choice', 'question': 'Continue?', 'options': ['Yes', 'No']}]})
            rpc_id += 1
            ask(rpc_id, 'item/commandExecution/requestApproval', {'reason': 'Run fixture command?'})
            # Keep this turn active so a second send must use turn/steer.
            continue
        if turn == 2:
            rpc_id += 1
            ask(rpc_id, 'item/fileChange/requestApproval', {'reason': 'Apply fixture change?'})
            notification('item/completed', {'item': {'type': 'agentMessage', 'id': 'assistant-2', 'text': 'second answer'}})
            notification('turn/completed', {'turn': {'id': turn_id, 'status': 'completed'}})
        elif turn == 3:
            # Leave this turn pending until the client asks to stop it.
            continue
    elif method == 'turn/steer':
        if scenario == 'steer-failed':
            emit({'id':ident,'error':{'code':-32000,'message':'Unavailable'}})
            continue
        if scenario == 'steer-race':
            notification('turn/completed', {'turn': {'id': params.get('expectedTurnId', 'fixture-turn-1'), 'status': 'completed'}})
            emit({'id':ident,'error':{'code':-32000,'message':'No active turn'}})
            continue
        reply(ident, {})
        notification('item/agentMessage/delta', {'itemId': 'assistant-1', 'delta': 'follow-up'})
        notification('item/completed', {'item': {'type': 'agentMessage', 'id': 'assistant-1', 'text': 'stream-1follow-up'}})
        notification('turn/completed', {'turn': {'id': params.get('expectedTurnId', 'fixture-turn-1'), 'status': 'completed'}})
    elif method == 'turn/interrupt':
        reply(ident, {})
        notification('turn/completed', {'turn': {'id': params['turnId'], 'status': 'interrupted'}})
        notification('thread/name/updated', {'threadName': 'After interrupt'})
    elif ident is not None:
        reply(ident, {})
"##;

const FAKE_CLAUDE: &str = r##"#!/usr/bin/python3
import json, os, signal, sys, time
log = os.environ['FAKE_PROVIDER_LOG']
count_path = os.environ['FAKE_CLAUDE_COUNT']
signal.alarm(35)
try:
    count = int(open(count_path, encoding='utf-8').read()) + 1
except (FileNotFoundError, ValueError):
    count = 1
with open(count_path, 'w', encoding='utf-8') as f:
    f.write(str(count))
def record(row):
    with open(log, 'a', encoding='utf-8') as f:
        f.write(json.dumps(row) + '\n')
        f.flush()
def emit(row):
    sys.stdout.write(json.dumps(row) + '\n')
    sys.stdout.flush()
args = sys.argv[1:]
record({'kind':'claude-start','pid':os.getpid(),'executable':os.path.realpath(sys.argv[0]),'args':args})
record({'kind':'pid','pid':os.getpid(),'executable':os.path.realpath(sys.argv[0])})
sys.stdin.read()
session_flag = '--resume' if '--resume' in args else '--session-id'
session = args[args.index(session_flag) + 1]
if count == 1:
    emit({'type':'stream_event','event':{'type':'message_start','message':{'id':'claude-message-one'}}})
    emit({'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'streamed first'}}})
    emit({'type':'assistant','message':{'content':[{'type':'text','text':'streamed first'}]},'session_id':session})
    emit({'type':'stream_event','event':{'type':'message_start','message':{'id':'claude-message-two'}}})
    emit({'type':'assistant','message':{'content':[{'type':'text','text':'second assistant message'}]},'session_id':session})
elif count == 2:
    emit({'type':'result','is_error':True,'result':'usage limit reached','session_id':session})
else:
    emit({'type':'stream_event','event':{'type':'message_start','message':{'id':'claude-cancel'}}})
    emit({'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'waiting to cancel'}}})
    time.sleep(25)
"##;

const FAKE_NACRE_AI_CHAT: &str = r##"#!/usr/bin/python3
import json, os, sys
with open(os.environ['FAKE_PROVIDER_LOG'], 'a', encoding='utf-8') as f:
    f.write(json.dumps({'kind':'configure','args':sys.argv[1:],'codex_home':os.environ.get('CODEX_HOME'),'history_home':os.environ.get('NACRE_AI_HISTORY_HOME')}) + '\n')
"##;

struct Fixture {
    home: TempDir,
    runtime: PathBuf,
    log: PathBuf,
    turn_state: PathBuf,
    provider_count: PathBuf,
    claude_count: PathBuf,
    resume_ready: PathBuf,
    resume_release: PathBuf,
    resume_ready_socket: UnixDatagram,
    tools: PathBuf,
    extras_log: PathBuf,
    extras_ready: PathBuf,
    extras_ready_socket: UnixDatagram,
    server: Option<Child>,
    channel: String,
    scenario: String,
}

impl Fixture {
    fn new(channel: &str) -> Self {
        Self::new_with_provider_and_scenario(channel, "codex", "chat")
    }

    fn new_with_provider(channel: &str, provider: &str) -> Self {
        Self::new_with_provider_and_scenario(channel, provider, "chat")
    }

    fn new_with_provider_and_scenario(channel: &str, provider: &str, scenario: &str) -> Self {
        let home = tempfile::tempdir().expect("fixture home");
        let runtime = home.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        executable(&bin.join("codex"), FAKE_CODEX);
        executable(&bin.join("nacre-ai"), "#!/bin/sh\nexit 0\n");
        executable(&bin.join("claude"), FAKE_CLAUDE);
        executable(&bin.join("nacre-ai-claude"), "#!/bin/sh\nexit 0\n");
        executable(&bin.join("nacre-ai-chat"), FAKE_NACRE_AI_CHAT);
        let tools = home.path().join("fixture-tools");
        fs::create_dir(&tools).unwrap();
        let extras_log = home.path().join("extras.jsonl");
        let extras_ready = home.path().join("extras-ready");
        let extras_ready_socket = UnixDatagram::bind(&extras_ready).expect("extras event socket");
        extras_ready_socket
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        executable(
            &tools.join("desktop-extras.py"),
            r##"#!/usr/bin/python3
import json, os, socket, sys
data = json.load(sys.stdin)
log = os.environ['FAKE_PROVIDER_LOG']
pid = None
for line in open(log, encoding='utf-8'):
    try:
        row = json.loads(line)
        if row.get('kind') == 'pid': pid = row['pid']
    except ValueError: pass
with open(os.environ['FAKE_EXTRAS_LOG'], 'a', encoding='utf-8') as f:
    f.write(json.dumps({'action':'resume','argv':sys.argv[1:],'key':data.get('key'),'provider_closed':not os.path.exists('/proc/' + str(pid))}) + '\n')
socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM).sendto(b'completed', os.environ['FAKE_EXTRAS_SOCKET'])
"##,
        );
        let config = home.path().join(".config/nacre/ai");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            format!(r#"{{"default_assistant":"{provider}","version":1}}"#),
        )
        .unwrap();
        fs::write(
            home.path().join(".config/nacre/profile.json"),
            r#"{"aiFullAccess":true}"#,
        )
        .unwrap();
        let log = home.path().join("provider.jsonl");
        let turn_state = home.path().join("turn-count");
        let provider_count = home.path().join("provider-count");
        let claude_count = home.path().join("claude-count");
        let resume_ready = home.path().join("resume-ready");
        let resume_release = home.path().join("resume-release");
        let resume_ready_socket = UnixDatagram::bind(&resume_ready).expect("resume event socket");
        resume_ready_socket
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let mut fixture = Self {
            home,
            runtime,
            log,
            turn_state,
            provider_count,
            claude_count,
            resume_ready,
            resume_release,
            resume_ready_socket,
            tools,
            extras_log,
            extras_ready,
            extras_ready_socket,
            server: None,
            channel: channel.into(),
            scenario: scenario.into(),
        };
        fixture.start_server();
        fixture
    }

    fn helper(&self, mode: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
        command
            .args(["sidebar-chat", mode, "--channel", &self.channel])
            .env_clear()
            .env("HOME", self.home.path())
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env("PATH", "/usr/bin:/bin")
            .env("FAKE_PROVIDER_LOG", &self.log)
            .env("FAKE_TURN_STATE", &self.turn_state)
            .env("FAKE_PROVIDER_COUNT", &self.provider_count)
            .env("FAKE_CLAUDE_COUNT", &self.claude_count)
            .env("FAKE_RESUME_READY", &self.resume_ready)
            .env("FAKE_RESUME_RELEASE", &self.resume_release)
            .env("FAKE_EXTRAS_SOCKET", &self.extras_ready)
            .env("FAKE_EXTRAS_LOG", &self.extras_log)
            .env("FAKE_SCENARIO", &self.scenario)
            .env("NACRE_HELPER_TOOLS", &self.tools)
            .stderr(Stdio::inherit());
        command
    }

    fn start_server(&mut self) {
        let child = self
            .helper("server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .expect("start native fixture server");
        self.server = Some(child);
    }

    fn socket(&self) -> PathBuf {
        self.runtime
            .join(format!("nacre-chat-{}.sock", self.channel))
    }

    fn seed_codex_metadata(&self, answered_questions: &[&str]) -> PathBuf {
        let root = self
            .home
            .path()
            .join(".local/state/nacre/shell/sidebar/views")
            .join(&self.channel);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let cwd = self.home.path().join(".local/share/nacre/ai/chats/fixture");
        fs::create_dir_all(&cwd).unwrap();
        fs::set_permissions(&cwd, fs::Permissions::from_mode(0o700)).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"codex|"));
        let path = root.join(format!("{}.json", &hash[..20]));
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "agent":"codex","account":"","id":"fixture-thread","cwd":cwd,
                "title":"Fixture","model":"","started":false,
                "answered_questions":answered_questions
            }))
            .unwrap(),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    fn metadata_path(&self) -> PathBuf {
        let root = self
            .home
            .path()
            .join(".local/state/nacre/shell/sidebar/views")
            .join(&self.channel);
        fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .expect("sidebar metadata file")
    }

    fn connect(&self) -> Client {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Ok(stream) = UnixStream::connect(self.socket()) {
                stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
                let reader = BufReader::new(stream.try_clone().unwrap());
                return Client { stream, reader };
            }
            assert!(
                Instant::now() < deadline,
                "sidebar socket did not become ready"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn kill_server(&mut self) {
        if let Some(mut child) = self.server.take() {
            let _ = child.kill();
            wait_child(&mut child);
        }
    }

    fn methods(&self) -> Vec<(String, Value)> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|row| row["kind"] == "input")
            .filter_map(|row| {
                Some((
                    row["row"]["method"].as_str()?.to_owned(),
                    row["row"]["params"].clone(),
                ))
            })
            .collect()
    }

    fn rpc_answers(&self) -> Vec<Value> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|row| row["kind"] == "rpc-answer")
            .map(|row| row["result"].clone())
            .collect()
    }

    fn provider_pids(&self) -> Vec<i32> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|row| row["kind"] == "pid")
            .filter_map(|row| row["pid"].as_i64().and_then(|pid| i32::try_from(pid).ok()))
            .collect()
    }

    fn fake_codex_is_running(&self) -> bool {
        let expected = self
            .home
            .path()
            .join(".local/bin/codex")
            .to_string_lossy()
            .into_owned();
        self.provider_pids().into_iter().any(|pid| {
            fs::read(format!("/proc/{pid}/cmdline"))
                .ok()
                .is_some_and(|line| {
                    line.windows(expected.len())
                        .any(|part| part == expected.as_bytes())
                })
        })
    }

    fn cleanup_fake_providers(&self) {
        for line in fs::read_to_string(&self.log).unwrap_or_default().lines() {
            let Ok(row) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let executable = row["executable"].as_str().unwrap_or("");
            let codex = self
                .home
                .path()
                .join(".local/bin/codex")
                .to_string_lossy()
                .into_owned();
            let claude = self
                .home
                .path()
                .join(".local/bin/claude")
                .to_string_lossy()
                .into_owned();
            if row["kind"] != "pid" || executable != codex && executable != claude {
                continue;
            }
            let Some(pid) = row["pid"].as_i64().and_then(|pid| i32::try_from(pid).ok()) else {
                continue;
            };
            let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            if !cmdline
                .windows(executable.len())
                .any(|window| window == executable.as_bytes())
            {
                continue;
            }
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            while Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if Path::new(&format!("/proc/{pid}")).exists() {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!(
                "sidebar fixture failure: channel={} scenario={} home={}",
                self.channel,
                self.scenario,
                self.home.path().display()
            );
            match fs::read_to_string(&self.log) {
                Ok(log) => eprintln!("fixture provider log:\n{log}"),
                Err(error) => eprintln!("fixture provider log unavailable: {error}"),
            }
            let state_dir = self
                .home
                .path()
                .join(".local/state/nacre/shell/sidebar/views")
                .join(&self.channel);
            match fs::read_dir(state_dir) {
                Ok(entries) => {
                    for entry in entries.flatten().filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "json")
                    }) {
                        match fs::read_to_string(entry.path()) {
                            Ok(metadata) => eprintln!(
                                "fixture sidebar metadata {}:\n{metadata}",
                                entry.path().display()
                            ),
                            Err(error) => eprintln!("fixture metadata unavailable: {error}"),
                        }
                    }
                }
                Err(error) => eprintln!("fixture metadata directory unavailable: {error}"),
            }
            match self.server.as_mut().map(|child| child.try_wait()) {
                Some(Ok(Some(status))) => eprintln!("fixture server status: exited {status}"),
                Some(Ok(None)) => eprintln!("fixture server status: running"),
                Some(Err(error)) => eprintln!("fixture server status unavailable: {error}"),
                None => eprintln!("fixture server status: not started"),
            }
        }
        self.cleanup_fake_providers();
        self.kill_server();
    }
}

struct Client {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}
impl Client {
    fn send(&mut self, action: Value) {
        serde_json::to_writer(&mut self.stream, &action).unwrap();
        self.stream.write_all(b"\n").unwrap();
        self.stream.flush().unwrap();
    }
    #[track_caller]
    fn next(&mut self, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => panic!("sidebar socket closed"),
                Ok(_) => return serde_json::from_str(&line).expect("valid sidebar JSON frame"),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for sidebar frame"
                    );
                }
                Err(error) => panic!("sidebar read failed: {error}"),
            }
        }
    }
    #[track_caller]
    fn matching(&mut self, timeout: Duration, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "timed out waiting for matching sidebar frame"
            );
            let event = self.next(left);
            if predicate(&event) {
                return event;
            }
        }
    }
    fn history(&mut self) -> Value {
        self.matching(Duration::from_secs(60), |event| event["type"] == "history")
    }
    #[track_caller]
    fn question(&mut self, kind: &str) -> Value {
        self.matching(Duration::from_secs(60), |event| {
            event["type"] == "state" && event["question"]["kind"] == kind
        })["question"]
            .clone()
    }
    #[track_caller]
    fn wait_idle(&mut self) -> Value {
        self.matching(Duration::from_secs(60), |event| {
            event["type"] == "state" && event["busy"] == false
        })
    }
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn wait_child(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().expect("poll child").is_some() {
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().expect("reap owned child");
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn accept_until(listener: &UnixListener, timeout: Duration) -> UnixStream {
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for relay client connection"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("relay accept failed: {error}"),
        }
    }
}

fn stdout_lines(stdout: std::process::ChildStdout) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if sender.send(line.clone()).is_err() => break,
                Ok(_) => {}
            }
        }
    });
    receiver
}

fn stdout_line(lines: &Receiver<String>) -> String {
    lines
        .recv_timeout(Duration::from_secs(60))
        .expect("timed out waiting for native client stdout")
}

fn client_after_handshake(path: &Path, timeout: Duration) -> Option<Client> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let Ok(stream) = UnixStream::connect(path) else {
            thread::sleep(Duration::from_millis(20));
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
        let Ok(reader_stream) = stream.try_clone() else {
            continue;
        };
        let mut reader = BufReader::new(reader_stream);
        let mut history = String::new();
        let mut state = String::new();
        let result = reader
            .read_line(&mut history)
            .and_then(|_| reader.read_line(&mut state));
        if result.is_ok()
            && serde_json::from_str::<Value>(&history).is_ok_and(|row| row["type"] == "history")
            && serde_json::from_str::<Value>(&state).is_ok_and(|row| row["type"] == "state")
        {
            return Some(Client { stream, reader });
        }
        drop(reader);
        drop(stream);
        thread::sleep(Duration::from_millis(20));
    }
    None
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn native_sidebar_preserves_stream_approval_steer_resume_and_channel_contract() {
    const FIRST_PROMPT: &str =
        "first prompt preserves only a short title then SECRET-TRANSCRIPT-BODY";
    let mut fixture = Fixture::new("fixture");
    let socket = fixture.socket();
    let mut client = fixture.connect();
    assert_eq!(
        fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(client.history()["messages"], json!([]));
    let state = client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(state["provider"], "codex");
    assert_eq!(state["threadId"], "");
    let state_dir = fixture
        .home
        .path()
        .join(".local/state/nacre/shell/sidebar/views/fixture");
    assert_eq!(
        fs::metadata(&state_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let metadata_path = fixture.metadata_path();
    assert_eq!(
        fs::metadata(&metadata_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let metadata: Value = serde_json::from_slice(&fs::read(metadata_path).unwrap()).unwrap();
    assert_eq!(
        fs::metadata(metadata["cwd"].as_str().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );

    client.send(json!({"action":"send","text":FIRST_PROMPT}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "stream-1"
    });
    let input = client.question("input");
    assert_eq!(input["questions"][0]["question"], "Continue?");
    client.send(
        json!({"action":"answer","id":"old-request","answers":{"choice":{"answers":["Wrong"]}}}),
    );
    client
        .send(json!({"action":"answer","id":input["id"],"answers":{"choice":{"answers":["Yes"]}}}));
    let approval = client.question("approval");
    assert_eq!(
        approval["questions"][0]["options"][0]["label"],
        "Allow once"
    );
    client.send(json!({"action":"answer","id":approval["id"],"answers":{"approval":{"answers":["Allow once"]}}}));
    client.send(json!({"action":"send","text":"follow-up prompt"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "follow-up"
    });
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "stream-1follow-up"
    });
    client.wait_idle();

    let methods = fixture.methods();
    let start = methods
        .iter()
        .find(|(method, _)| method == "thread/start")
        .expect("Codex starts a thread");
    assert_eq!(start.1["approvalPolicy"], "never");
    assert_eq!(start.1["sandbox"], "danger-full-access");
    assert!(
        methods.iter().any(|(method, params)| method == "turn/steer"
            && params["expectedTurnId"] == "fixture-turn-1"),
        "busy follow-up steers the active Codex turn"
    );
    assert!(fixture
        .rpc_answers()
        .iter()
        .any(|answer| answer["answers"]["choice"]["answers"][0] == "Yes"));
    assert!(fixture
        .rpc_answers()
        .iter()
        .any(|answer| answer["decision"] == "accept"));
    let metadata: Value =
        serde_json::from_slice(&fs::read(fixture.metadata_path()).unwrap()).unwrap();
    let serialized = metadata.to_string();
    assert!(!serialized.contains("SECRET-TRANSCRIPT-BODY"));
    assert!(!serialized.contains("stream-1follow-up"));

    let mut reconnect = fixture.connect();
    let history = reconnect.history();
    let rows = history["messages"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        3,
        "native echoes and completion update existing rows"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["role"] == "user" && row["text"] == FIRST_PROMPT)
            .count(),
        1
    );
    assert_eq!(
        rows.iter().filter(|row| row["id"] == "assistant-1").count(),
        1
    );
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["role"] == "user" && row["text"] == FIRST_PROMPT));
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["role"] == "assistant" && row["text"] == "stream-1follow-up"));
    drop(reconnect);

    drop(client);
    fixture.cleanup_fake_providers();
    fixture.kill_server();
    fixture.start_server();
    let mut client = fixture.connect();
    assert_eq!(client.history()["messages"], json!([]));
    let restored = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "history"
            && event["messages"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == "old-user"))
    });
    assert!(restored["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["text"] == "earlier answer"));
    let resumed_state = client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(resumed_state["threadId"], "fixture-thread");
    client.send(json!({"action":"send","text":"second turn"}));
    assert!(fixture.methods().iter().any(
        |(method, params)| method == "thread/resume" && params["threadId"] == "fixture-thread"
    ));
    let decline = client.question("approval");
    client.send(json!({"action":"answer","id":decline["id"],"answers":{"approval":{"answers":["Decline"]}}}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "second answer"
    });
    client.wait_idle();
    assert!(fixture
        .rpc_answers()
        .iter()
        .any(|answer| answer["decision"] == "decline"));

    client.send(json!({"action":"send","text":"interrupt me"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "stream-3"
    });
    client.send(json!({"action":"stop"}));
    let stopped = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["title"] == "After interrupt"
    });
    assert!(!stopped["busy"].as_bool().unwrap());
    assert!(
        fixture
            .methods()
            .iter()
            .any(|(method, params)| method == "turn/interrupt"
                && params["turnId"] == "fixture-turn-3"),
        "stop interrupts the active Codex turn"
    );

    fixture.cleanup_fake_providers();
    fixture.kill_server();
    fixture.channel = "other".into();
    fixture.start_server();
    let mut other = fixture.connect();
    assert_eq!(other.history()["messages"], json!([]));
    let other_state = other.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(other_state["threadId"], "");
    assert!(fixture
        .home
        .path()
        .join(".local/state/nacre/shell/sidebar/views/other")
        .is_dir());
    other.send(json!([]));
    other.send(json!({"action":"provider","provider":"claude"}));
    assert_eq!(
        other.matching(Duration::from_secs(60), |event| event["type"] == "state")["type"],
        "state"
    );
    other
        .stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let mut oversized = vec![b'x'; 1_048_577];
    oversized.push(b'\n');
    let _ = other.stream.write_all(&oversized);
    let mut closed = [0u8; 1];
    assert_eq!(
        other.stream.read(&mut closed).unwrap(),
        0,
        "oversized line closes only its client socket"
    );
}

#[test]
fn native_claude_peer_preserves_streams_resume_usage_errors_and_cancellation() {
    let fixture = Fixture::new_with_provider("claude-fixture", "claude");
    let mut client = fixture.connect();
    assert_eq!(client.history()["messages"], json!([]));
    let state = client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(state["provider"], "claude");

    client.send(json!({"action":"send","text":"first Claude prompt"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "streamed first"
    });
    let second_message = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "second assistant message"
    });
    assert_eq!(second_message["id"], "claude-message-two");
    client.wait_idle();

    let log: Vec<Value> = fs::read_to_string(&fixture.log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let first = log
        .iter()
        .find(|row| row["kind"] == "claude-start")
        .expect("fake Claude invocation");
    let args = first["args"].as_array().unwrap();
    assert!(args
        .iter()
        .any(|arg| arg == "--dangerously-skip-permissions"));
    assert!(
        args.iter().any(|arg| arg == "--output-format")
            && args.iter().any(|arg| arg == "stream-json")
    );
    let session_arg = args
        .iter()
        .position(|arg| arg == "--session-id")
        .expect("new Claude session flag");
    let session_id = args[session_arg + 1].as_str().expect("session ID");
    assert!(!session_id.is_empty());
    assert!(!args.iter().any(|arg| arg == "--resume"));

    let state_dir = fixture
        .home
        .path()
        .join(".local/state/nacre/shell/sidebar/views/claude-fixture");
    let metadata_path = fs::read_dir(&state_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("Claude metadata file");
    let metadata: Value = serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(metadata["id"], session_id);
    assert_eq!(metadata["started"], true);

    client.send(json!({"action":"send","text":"limit prompt"}));
    let usage_state = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && !event["error"].as_str().unwrap_or("").is_empty()
    });
    assert_eq!(
        usage_state["error"],
        "The selected assistant account has reached its current usage limit."
    );
    assert!(usage_state["busy"] == false);
    assert!(usage_state["question"].is_null());
    let mut reconnect = fixture.connect();
    let usage_history = reconnect.history();
    assert!(usage_history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| { row["role"] == "user" && row["text"] == "limit prompt" }));
    assert!(usage_history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| { row["role"] == "assistant" && row["text"] == "second assistant message" }));
    let log: Vec<Value> = fs::read_to_string(&fixture.log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let second = log
        .iter()
        .filter(|row| row["kind"] == "claude-start")
        .nth(1)
        .expect("resumed fake Claude invocation");
    let resume_arg = second["args"]
        .as_array()
        .unwrap()
        .iter()
        .position(|arg| arg == "--resume")
        .expect("Claude resume flag");
    assert_eq!(second["args"][resume_arg + 1], session_id);

    client.send(json!({"action":"send","text":"cancel prompt"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "waiting to cancel"
    });
    client.send(json!({"action":"stop"}));
    let interrupted = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["error"] == "Response interrupted"
    });
    assert!(!interrupted["busy"].as_bool().unwrap());
    let mut after_interrupt = fixture.connect();
    let history = after_interrupt.history();
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| { row["role"] == "user" && row["text"] == "cancel prompt" }));
}

#[test]
fn native_sidebar_async_question_reset_answer_and_suppression_match_history_contract() {
    let mut fixture = Fixture::new_with_provider_and_scenario("async", "codex", "async");
    fixture.kill_server();
    fixture.seed_codex_metadata(&[]);
    fixture.start_server();

    let mut client = fixture.connect();
    assert_eq!(client.history()["messages"], json!([]));
    let question_state = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["question"]["kind"] == "async"
    });
    assert_eq!(question_state["busy"], false);
    let question = question_state["question"].clone();
    assert_eq!(question["id"], "async-new-question");
    assert_eq!(question["questions"][0]["question"], "New question");
    client.send(
        json!({"action":"answer","id":"stale-question","answers":{"0":{"answers":["Wrong"]}}}),
    );
    client.send(json!({"action":"answer","id":question["id"],"answers":{"0":{"answers":["B"]}}}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message" && event["role"] == "assistant" && event["text"] == "async reply"
    });
    let blocking = client.question("input");
    assert_eq!(blocking["questions"][0]["header"], "Keep both");
    client.send(
        json!({"action":"answer","id":blocking["id"],"answers":{"coexist":{"answers":["Yes"]}}}),
    );
    let live_question = client.wait_idle();
    assert_eq!(live_question["question"]["id"], "async-live-question-item");
    assert_eq!(
        live_question["busy"], false,
        "async question survives the completed provider turn"
    );
    assert_eq!(
        live_question["question"]["questions"][0]["question"],
        "Question after finish?"
    );
    let turn = fixture
        .methods()
        .into_iter()
        .find(|(method, _)| method == "turn/start")
        .expect("async answer starts a provider turn")
        .1;
    assert_eq!(turn["input"][0]["text"], "New question\nAnswer: B");
    let saved: Value = serde_json::from_slice(&fs::read(fixture.metadata_path()).unwrap()).unwrap();
    assert!(saved["answered_questions"]
        .as_array()
        .unwrap()
        .contains(&json!("async-new-question")));

    drop(client);
    fixture.cleanup_fake_providers();
    fixture.kill_server();
    fixture.start_server();
    let mut resumed = fixture.connect();
    let history = resumed.matching(Duration::from_secs(60), |event| {
        event["type"] == "history"
            && event["messages"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["text"] == "latest input"))
    });
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == "new-question"));
    let state = resumed.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["question"].is_null()
    });
    assert!(
        state["question"].is_null(),
        "answered async question stays suppressed after history restore"
    );
}

#[test]
fn native_workspace_handoff_closes_provider_before_resume_and_blocks_reconnect() {
    let fixture = Fixture::new_with_provider_and_scenario("workspace", "codex", "workspace");
    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    client.send(json!({"action":"send","text":"continue in terminal"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "workspace answer"
    });
    client.wait_idle();
    let original_pids = fixture.provider_pids().len();

    client.send(json!({"action":"workspace"}));
    let handoff = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["inWorkspace"] == true
    });
    assert!(handoff["status"]
        .as_str()
        .unwrap()
        .contains("Continue this chat in the workspace"));
    let mut event = [0; 64];
    let (length, _) = fixture.extras_ready_socket.recv_from(&mut event).unwrap();
    assert_eq!(&event[..length], b"completed");
    assert!(
        fixture.extras_log.exists(),
        "fake desktop extras ran after handoff state"
    );
    let extras: Vec<Value> = fs::read_to_string(&fixture.extras_log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(extras.len(), 1);
    assert_eq!(extras[0]["argv"], json!(["resume"]));
    assert_eq!(extras[0]["key"].as_str().unwrap().len(), 24);
    assert_eq!(extras[0]["provider_closed"], true);
    assert_eq!(fixture.provider_pids().len(), original_pids);

    client.send(json!({"action":"send","text":"must not reopen"}));
    let mut reconnect = fixture.connect();
    let history = reconnect.history();
    assert!(history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["text"] == "continue in terminal"));
    let reconnected = reconnect.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(reconnected["inWorkspace"], true);
    assert_eq!(
        fixture.provider_pids().len(),
        original_pids,
        "workspace-owned chat does not reconnect its Codex peer"
    );

    reconnect.send(json!({"action":"new"}));
    assert_eq!(reconnect.history()["messages"], json!([]));
    let fresh = reconnect.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["inWorkspace"] == false
    });
    assert_eq!(fresh["threadId"], "");
}

#[test]
fn native_busy_workspace_action_does_not_close_active_codex_peer() {
    let fixture = Fixture::new("busy-workspace");
    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    client.send(json!({"action":"send","text":"keep working"}));
    let question = client.question("input");
    client.send(json!({"action":"workspace"}));
    thread::sleep(Duration::from_millis(150));
    assert!(
        fixture.fake_codex_is_running(),
        "busy workspace request leaves the provider process alive"
    );
    assert!(
        !fixture.extras_log.exists(),
        "busy workspace request does not launch desktop extras"
    );
    assert_eq!(question["questions"][0]["question"], "Continue?");
}

#[test]
fn native_provider_default_changes_without_converting_current_chat() {
    let fixture = Fixture::new("provider-choice");
    let mut client = fixture.connect();
    client.history();
    let state = client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(state["provider"], "codex");
    client.send(json!({"action":"provider","provider":"claude"}));
    let updated = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["defaultProvider"] == "claude"
    });
    assert_eq!(updated["provider"], "codex");
    let settings: Value = serde_json::from_slice(
        &fs::read(fixture.home.path().join(".config/nacre/ai/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["default_assistant"], "claude");
    assert!(
        fixture.provider_pids().is_empty(),
        "changing the default does not start or convert a provider session"
    );
}

#[test]
fn native_failed_steer_restores_draft_without_finishing_active_turn() {
    let fixture = Fixture::new_with_provider_and_scenario("steer-failed", "codex", "steer-failed");
    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    client.send(json!({"action":"send","text":"original active turn"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "stream-1"
    });
    client.send(json!({"action":"send","text":"Keep this draft"}));
    let failed = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "sendFailed" && event["text"] == "Keep this draft"
    });
    assert_eq!(failed["attachments"], json!([]));
    let state = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["busy"] == true
    });
    assert!(
        state["busy"].as_bool().unwrap(),
        "failed steering preserves active turn busy state"
    );
    assert!(!state.to_string().contains("Keep this draft"));
    assert!(fixture
        .methods()
        .iter()
        .any(|(method, _)| method == "turn/steer"));
}

#[test]
fn native_completed_turn_steer_race_starts_one_followup_turn() {
    let fixture = Fixture::new_with_provider_and_scenario("steer-race", "codex", "steer-race");
    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    client.send(json!({"action":"send","text":"first turn"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "delta" && event["text"] == "stream-1"
    });
    client.send(json!({"action":"send","text":"follow up after completion"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "fallback answer"
    });
    client.wait_idle();
    let methods = fixture.methods();
    assert_eq!(
        methods
            .iter()
            .filter(|(method, _)| method == "turn/steer")
            .count(),
        1
    );
    assert_eq!(
        methods
            .iter()
            .filter(|(method, _)| method == "turn/start")
            .count(),
        2
    );
    assert_eq!(
        methods
            .iter()
            .filter(|(method, params)| method == "turn/start"
                && params["input"][0]["text"] == "follow up after completion")
            .count(),
        1
    );
}

#[test]
fn native_sidebar_client_discards_partial_frame_when_relay_reconnects() {
    let home = tempfile::tempdir().unwrap();
    let runtime = home.path().join("runtime");
    let fake_bin = home.path().join("fake-bin");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(&fake_bin).unwrap();
    let systemctl_log = home.path().join("systemctl.log");
    let systemd_run_log = home.path().join("systemd-run.log");
    executable(
        &fake_bin.join("systemctl"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FAKE_SYSTEMCTL_LOG\"\nexit 0\n",
    );
    executable(
        &fake_bin.join("systemd-run"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FAKE_SYSTEMD_RUN_LOG\"\nexit 99\n",
    );

    let socket_path = runtime.join("nacre-chat-relay.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let path = format!("{}:/usr/bin:/bin", fake_bin.display());
    let child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["sidebar-chat", "client", "--channel", "relay"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PATH", path)
        .env("FAKE_SYSTEMCTL_LOG", &systemctl_log)
        .env("FAKE_SYSTEMD_RUN_LOG", &systemd_run_log)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start native sidebar relay client");
    let mut child = ChildGuard(child);
    let stdin = child.0.stdin.take().expect("client stdin remains open");
    let lines = stdout_lines(child.0.stdout.take().expect("client stdout"));

    let mut first = accept_until(&listener, Duration::from_secs(60));
    first
        .write_all(br#"{"type":"history","messages":["#)
        .unwrap();
    first.flush().unwrap();
    drop(first);

    let reconnect_line = stdout_line(&lines);
    let reconnect: Value =
        serde_json::from_str(&reconnect_line).expect("reconnect is one complete JSON frame");
    assert_eq!(reconnect["type"], "connection");
    assert_eq!(reconnect["connected"], false);

    let mut second = accept_until(&listener, Duration::from_secs(60));
    second
        .write_all(b"{\"type\":\"history\",\"messages\":[{\"id\":\"fresh\",\"role\":\"user\",\"text\":\"reconnected history\"}]}\n{\"type\":\"state\",\"provider\":\"codex\",\"busy\":false}\n")
        .unwrap();
    second.flush().unwrap();
    let history_line = stdout_line(&lines);
    let state_line = stdout_line(&lines);
    let history: Value = serde_json::from_str(&history_line).expect("complete history JSON frame");
    let state: Value = serde_json::from_str(&state_line).expect("complete state JSON frame");
    assert_eq!(history["type"], "history");
    assert_eq!(history["messages"][0]["text"], "reconnected history");
    assert_eq!(state["type"], "state");
    assert_eq!(state["busy"], false);
    assert!(systemctl_log.exists());
    assert_eq!(
        fs::read_to_string(&systemctl_log).unwrap().trim(),
        "--user is-active --quiet nacre-sidebar-ai-view-relay"
    );
    assert!(
        !systemd_run_log.exists(),
        "fixture systemctl prevents any systemd-run invocation"
    );

    drop(stdin);
    wait_child(&mut child.0);
    assert!(child.0.wait().unwrap().success());
    match lines.recv_timeout(Duration::from_secs(60)) {
        Err(mpsc::RecvTimeoutError::Disconnected) => {}
        Ok(extra) => panic!("unexpected extra native client stdout line: {extra:?}"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("native client stdout reader did not close"),
    }
    drop(second);
}

#[test]
fn native_named_codex_account_configures_isolated_home_before_provider_start() {
    let mut fixture =
        Fixture::new_with_provider_and_scenario("named-account", "codex", "workspace");
    fixture.kill_server();
    fs::write(
        fixture.home.path().join(".config/nacre/ai/settings.json"),
        r#"{"default_assistant":"codex","accounts":{"codex":"work"},"version":1}"#,
    )
    .unwrap();
    fixture.start_server();

    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");
    client.send(json!({"action":"send","text":"named account fixture"}));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "workspace answer"
    });
    client.wait_idle();

    let records: Vec<Value> = fs::read_to_string(&fixture.log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let configured = records
        .iter()
        .position(|row| row["kind"] == "configure")
        .expect("fixture nacre-ai-chat configure call");
    let provider = records
        .iter()
        .position(|row| row["kind"] == "pid")
        .expect("fake Codex process");
    let initialize = records
        .iter()
        .position(|row| row["kind"] == "input" && row["row"]["method"] == "initialize")
        .expect("Codex initialize request");
    assert!(
        configured < provider && provider < initialize,
        "account configuration precedes Codex handshake"
    );

    let account_home = fixture
        .home
        .path()
        .join(".local/share/nacre/ai/accounts/work");
    let configure = &records[configured];
    assert_eq!(configure["args"], json!(["configure"]));
    assert_eq!(
        configure["codex_home"],
        account_home.to_string_lossy().as_ref()
    );
    assert!(
        configure["history_home"].is_null(),
        "history override is removed for account setup"
    );
    assert_eq!(
        records[provider]["code_home"],
        account_home.to_string_lossy().as_ref()
    );
    assert!(
        !account_home.join("auth.json").exists(),
        "fixture account contains no credentials"
    );

    let metadata: Value =
        serde_json::from_slice(&fs::read(fixture.metadata_path()).unwrap()).unwrap();
    assert_eq!(metadata["account"], "work");
    assert!(Path::new(metadata["cwd"].as_str().unwrap())
        .starts_with(fixture.home.path().join(".local/share/nacre/ai/chats")));
}

#[test]
fn native_sidebar_server_caps_clients_and_reclaims_disconnected_slots() {
    let fixture = Fixture::new("client-cap");
    let mut clients = Vec::new();
    for _ in 0..32 {
        let mut client = fixture.connect();
        assert_eq!(client.history()["type"], "history");
        assert_eq!(
            client.matching(Duration::from_secs(60), |event| event["type"] == "state")["type"],
            "state"
        );
        clients.push(client);
    }

    // Intended security difference: clients over the 32-connection cap are closed without a handshake.
    let mut rejected = UnixStream::connect(fixture.socket()).expect("bounded client-count probe");
    rejected
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut byte = [0u8; 1];
    loop {
        match rejected.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => panic!("over-cap client unexpectedly received a handshake byte"),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && Instant::now() < deadline => {}
            Err(error) => panic!("over-cap client did not close cleanly: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "over-cap client was not closed before deadline"
        );
    }
    drop(rejected);

    clients.clear();
    let _reclaimed = client_after_handshake(&fixture.socket(), Duration::from_secs(60))
        .expect("server reclaims a client slot after disconnect");
    assert!(
        fixture.provider_pids().is_empty(),
        "capacity test never launches a provider"
    );
}

#[test]
fn native_new_chat_fences_late_background_resume() {
    let mut fixture =
        Fixture::new_with_provider_and_scenario("resume-race", "codex", "resume-race");
    fixture.kill_server();
    let old_metadata_path = fixture.seed_codex_metadata(&[]);
    let old_metadata: Value =
        serde_json::from_slice(&fs::read(&old_metadata_path).unwrap()).unwrap();
    let old_cwd = old_metadata["cwd"].as_str().unwrap().to_owned();
    fixture.start_server();

    let mut client = fixture.connect();
    assert_eq!(
        client.matching(Duration::from_secs(60), |event| event["type"] == "history")["messages"],
        json!([])
    );
    let mut event = [0; 64];
    let (length, _) = fixture.resume_ready_socket.recv_from(&mut event).unwrap();
    assert_eq!(
        &event[..length],
        b"ready",
        "old provider reached the blocked resume response"
    );

    client.send(json!({"action":"new"}));
    let fresh_state = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state" && event["threadId"] == "" && event["inWorkspace"] == false
    });
    assert_eq!(fresh_state["title"], "New chat");
    let new_metadata: Value =
        serde_json::from_slice(&fs::read(fixture.metadata_path()).unwrap()).unwrap();
    let composer_key = format!(
        "{:x}",
        Sha256::digest(new_metadata["cwd"].as_str().unwrap().as_bytes())
    );
    assert_eq!(fresh_state["composerKey"], composer_key);
    assert_eq!(new_metadata["id"], "");
    assert_ne!(new_metadata["cwd"], old_cwd);

    UnixDatagram::unbound()
        .unwrap()
        .send_to(b"release", &fixture.resume_release)
        .unwrap();
    // Once the reply is flushed, the server can reject the stale generation
    // and SIGTERM the provider before its next instruction. Observe that drop
    // decision explicitly instead of requiring a post-reply log write.
    let (length, _) = fixture.resume_ready_socket.recv_from(&mut event).unwrap();
    assert_eq!(
        &event[..length],
        b"peer-dropped",
        "server terminated the stale provider after resume release"
    );
    assert!(
        fs::read_to_string(&fixture.log)
            .unwrap()
            .contains("resume-response-released"),
        "stale provider was terminated before its delayed resume response was released"
    );
    let provider_deadline = Instant::now() + Duration::from_secs(60);
    while fixture.fake_codex_is_running() && Instant::now() < provider_deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !fixture.fake_codex_is_running(),
        "stale peer is dropped after its late resume response"
    );

    let mut reconnect = fixture.connect();
    assert_eq!(reconnect.history()["messages"], json!([]));
    let state = reconnect.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(state["threadId"], "");
    assert_eq!(state["composerKey"], composer_key);
    let saved: Value = serde_json::from_slice(&fs::read(fixture.metadata_path()).unwrap()).unwrap();
    assert_eq!(saved["id"], "");
    assert_eq!(saved["cwd"], new_metadata["cwd"]);

    reconnect.send(json!({"action":"send","text":"fresh chat prompt"}));
    reconnect.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "fresh chat answer"
    });
    let methods = fixture.methods();
    assert_eq!(
        methods
            .iter()
            .filter(|(method, _)| method == "thread/resume")
            .count(),
        1
    );
    let starts: Vec<_> = methods
        .iter()
        .filter(|(method, _)| method == "thread/start")
        .collect();
    assert_eq!(starts.len(), 1, "fresh send starts a new provider thread");
    assert_eq!(starts[0].1["cwd"], new_metadata["cwd"]);
}

#[test]
fn native_reconnected_blocking_question_survives_stale_waiter() {
    let fixture = Fixture::new_with_provider_and_scenario(
        "question-reconnect",
        "codex",
        "question-reconnect",
    );
    let mut client = fixture.connect();
    client.history();
    client.matching(Duration::from_secs(60), |event| event["type"] == "state");

    client.send(json!({"action":"send","text":"first request"}));
    let first_question = client.question("input");
    assert_eq!(first_question["id"], "900");
    assert_eq!(
        first_question["questions"][0]["question"],
        "Retry question?"
    );
    let ended = client.matching(Duration::from_secs(60), |event| {
        event["type"] == "state"
            && event["error"] == "The assistant connection ended; your conversation is saved."
    });
    assert!(!ended["busy"].as_bool().unwrap());

    client.send(json!({"action":"send","text":"retry request"}));
    let retried_question = client.question("input");
    assert_eq!(
        retried_question["id"], "900",
        "the restarted app-server reuses the RPC ID"
    );
    thread::sleep(Duration::from_millis(150));

    let mut observer = fixture.connect();
    observer.history();
    let observed_state =
        observer.matching(Duration::from_secs(60), |event| event["type"] == "state");
    assert_eq!(observed_state["question"]["kind"], "input");
    assert_eq!(observed_state["question"]["id"], "900");

    client.send(json!({
        "action":"answer",
        "id":retried_question["id"],
        "answers":{"choice":{"answers":["A"]}}
    }));
    client.matching(Duration::from_secs(60), |event| {
        event["type"] == "message"
            && event["role"] == "assistant"
            && event["text"] == "completed after retry"
    });
    assert!(fixture
        .rpc_answers()
        .iter()
        .any(|answer| { answer["answers"]["choice"]["answers"][0] == "A" }));
    let question_sends = fs::read_to_string(&fixture.log)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["kind"] == "question-sent")
        .collect::<Vec<_>>();
    assert_eq!(question_sends.len(), 2);
    assert!(question_sends.iter().all(|row| row["rpc_id"] == 900));
}

struct Worker {
    child: Child,
    input: Option<std::process::ChildStdin>,
    lines: Receiver<String>,
}
impl Worker {
    fn start(fixture: &mut Fixture) -> Self {
        // The fixture's socket server would own the channel lock.
        fixture.kill_server();
        let mut child = fixture
            .helper("worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start daemon worker");
        let input = child.stdin.take();
        let lines = stdout_lines(child.stdout.take().unwrap());
        Self {
            child,
            input,
            lines,
        }
    }
    fn send(&mut self, action: Value) {
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(&mut *input, &action).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }
    #[track_caller]
    fn matching(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        loop {
            let event: Value = serde_json::from_str(&stdout_line(&self.lines)).unwrap();
            if predicate(&event) {
                return event;
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.input.take();
        wait_child(&mut self.child);
    }
}

#[test]
fn daemon_worker_speaks_the_socket_contract_on_stdio_at_low_priority_and_stops_with_its_pipe() {
    let mut fixture = Fixture::new_with_provider("worker-fixture", "claude");
    let mut worker = Worker::start(&mut fixture);
    // The same handshake a socket client gets: history, then state.
    let first: Value = serde_json::from_str(&stdout_line(&worker.lines)).unwrap();
    assert_eq!(first, json!({"type":"history","messages":[]}));
    let state = worker.matching(|event| event["type"] == "state");
    assert_eq!(state["provider"], "claude");
    let stat = fs::read_to_string(format!("/proc/{}/stat", worker.child.id())).unwrap();
    let nice: i32 = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(16)
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(nice, 10);
    // A second owner of the channel is refused while the worker runs.
    let refused = fixture
        .helper("worker")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(!refused.success());
    worker.send(json!({"action":"send","text":"first Claude prompt"}));
    worker.matching(|event| event["type"] == "delta" && event["text"] == "streamed first");
    worker.matching(|event| event["type"] == "state" && event["busy"] == false);
    worker.send(json!({"action":"ping"}));
    worker.send(json!({"action":"send","text":"limit prompt"}));
    let limited = worker.matching(|event| {
        event["type"] == "state" && !event["error"].as_str().unwrap_or("").is_empty()
    });
    assert_eq!(
        limited["error"],
        "The selected assistant account has reached its current usage limit."
    );
    // Closing the pipe is the daemon's stop signal; no provider outlives it.
    worker.input.take();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = worker.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not stop with its pipe"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
}

#[test]
fn daemon_worker_stops_by_itself_after_thirty_idle_seconds() {
    let mut fixture = Fixture::new_with_provider("worker-idle", "claude");
    let mut worker = Worker::start(&mut fixture);
    worker.matching(|event| event["type"] == "state");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = worker.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(45),
            "idle worker did not stop"
        );
        thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success());
    assert!(
        started.elapsed() >= Duration::from_secs(29),
        "stopped before the idle limit"
    );
}
