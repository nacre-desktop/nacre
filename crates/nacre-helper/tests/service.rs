use nacre_helper::{
    json,
    service::{self, Limits},
};
use serde_json::{json as value, Value};
use std::{
    io::{self, Cursor, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Output(Arc<Mutex<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn packets(output: &Output) -> Vec<Value> {
    String::from_utf8(output.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn response_write_failures_are_returned_instead_of_swallowed() {
    struct ClosedAfterReady(bool);
    impl Write for ClosedAfterReady {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0 {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "fixture receiver closed",
                ))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0 = true;
            Ok(())
        }
    }
    let input = format!(
        "{}\n",
        value!({"id":1,"tool":"network-state","arguments":[],"input":null})
    );
    let error = service::serve(
        Cursor::new(input),
        ClosedAfterReady(false),
        Limits::default(),
        |_| Ok("fixture response".into()),
    )
    .expect_err("Worker write failure must propagate");
    assert_eq!(
        error
            .downcast_ref::<io::Error>()
            .expect("Original I/O error")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}
#[test]
fn preserves_json_file_spacing_order_and_ascii_escaping() {
    assert_eq!(
        json::encode(&value!({"z":"λ\n😀", "a":[1,2,true,null]})).unwrap(),
        r#"{"z": "\u03bb\n\ud83d\ude00", "a": [1, 2, true, null]}"#
    );
    assert_eq!(json::encode(&"\u{7f}").unwrap(), r#""\u007f""#);
}
#[test]
fn validates_ids_types_bounds_and_unicode_lengths() {
    let request = value!({"id":1,"tool":"network-state","arguments":[],"input":null});
    assert_eq!(service::validate(&request).unwrap().id, 1);
    for id in [
        value!(true),
        value!(0),
        value!(-1),
        value!(1.0),
        value!(9_007_199_254_740_992u64),
    ] {
        let mut invalid = request.clone();
        invalid["id"] = id;
        assert_eq!(
            service::validate(&invalid).unwrap_err().to_string(),
            "Invalid helper request ID"
        );
    }
    let mut unicode = request.clone();
    unicode["arguments"] = value!(["λ".repeat(65536)]);
    assert!(service::validate(&unicode).is_ok());
    unicode["arguments"] = value!(["λ".repeat(65537)]);
    assert_eq!(
        service::validate(&unicode).unwrap_err().to_string(),
        "Invalid helper arguments"
    );
}
#[test]
fn operation_errors_do_not_break_fifo_or_eof_drain() {
    let output = Output::default();
    let input = (1..=3)
        .map(|id| {
            format!(
                "{}\n",
                value!({"id":id,"tool":"network-state","arguments":[],"input":null})
            )
        })
        .collect::<String>();
    assert_eq!(
        service::serve(
            Cursor::new(input),
            output.clone(),
            Limits::default(),
            |request| {
                if request.id == 2 {
                    Err("fixture failure".into())
                } else {
                    Ok(format!("{}\n", request.id))
                }
            }
        )
        .unwrap(),
        0
    );
    let rows = packets(&output);
    assert_eq!(rows[0], value!({"ready":true,"version":1}));
    assert_eq!(rows[1], value!({"id":1,"code":0,"output":"1\n"}));
    assert_eq!(
        rows[2],
        value!({"id":2,"code":1,"output":r#"{"error": "fixture failure"}"#})
    );
    assert_eq!(rows[3], value!({"id":3,"code":0,"output":"3\n"}));
}
#[test]
fn bounded_frames_and_invalid_json_match_python_errors() {
    let output = Output::default();
    assert_eq!(
        service::serve(
            Cursor::new("{broken\n{\"id\":NaN}\n"),
            output.clone(),
            Limits::default(),
            |_| unreachable!()
        )
        .unwrap(),
        0
    );
    let rows = packets(&output);
    assert_eq!(
        rows[1]["output"],
        r#"{"error": "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"}"#
    );
    assert_eq!(rows[2]["output"], r#"{"error": "Non-finite JSON value"}"#);
    let output = Output::default();
    assert_eq!(
        service::serve(
            Cursor::new("x".repeat(33)),
            output.clone(),
            Limits {
                request: 32,
                ..Limits::default()
            },
            |_| unreachable!()
        )
        .unwrap(),
        1
    );
    assert_eq!(
        packets(&output)[1],
        value!({"fatal":"Helper request is too large"})
    );
}

#[cfg(unix)]
fn request(id: u64, tool: &str) -> String {
    format!(
        "{}\n",
        value!({"id":id,"tool":tool,"arguments":[],"input":null})
    )
}
#[cfg(unix)]
fn read_packet(reader: &mut impl std::io::BufRead) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
#[test]
#[cfg(unix)]
fn independent_lanes_do_not_wait_for_a_busy_helper() {
    use std::{
        io::{BufReader, Write},
        os::unix::net::UnixStream,
        sync::mpsc,
        thread,
    };
    let (mut client, server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let (release, gate) = mpsc::channel();
    let gate = Mutex::new(gate);
    let (started, observed_start) = mpsc::channel();
    let order = Arc::new(Mutex::new(Vec::new()));
    let observed_order = order.clone();
    let worker = thread::spawn(move || {
        service::serve(
            BufReader::new(server.try_clone().unwrap()),
            server,
            Limits::default(),
            move |request| {
                if request.tool == "dashboard-data" {
                    order.lock().unwrap().push(request.id);
                }
                if request.id == 1 {
                    started.send(()).unwrap();
                    gate.lock().unwrap().recv().unwrap();
                }
                Ok(format!("{}\n", request.id))
            },
        )
    });
    assert_eq!(read_packet(&mut reader), value!({"ready":true,"version":1}));
    write!(
        client,
        "{}{}{}{}{}",
        request(1, "dashboard-data"),
        request(2, "dashboard-data"),
        request(3, "dashboard-data"),
        request(4, "dashboard-data"),
        request(5, "network-state")
    )
    .unwrap();
    observed_start
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(read_packet(&mut reader)["id"], 5);
    assert_eq!(*observed_order.lock().unwrap(), vec![1]);
    release.send(()).unwrap();
    for id in 1..=4 {
        assert_eq!(read_packet(&mut reader)["id"], id);
    }
    assert_eq!(*observed_order.lock().unwrap(), vec![1, 2, 3, 4]);
    client.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(worker.join().unwrap().unwrap(), 0);
}
#[test]
#[cfg(unix)]
fn global_pending_bound_and_duplicate_ids_match_protocol() {
    use std::{
        io::{BufReader, Write},
        os::unix::net::UnixStream,
        sync::mpsc,
        thread,
    };
    for duplicate in [false, true] {
        let (mut client, server) = UnixStream::pair().unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let worker = thread::spawn(move || {
            service::serve(
                BufReader::new(server.try_clone().unwrap()),
                server,
                Limits {
                    pending: 2,
                    ..Limits::default()
                },
                move |request| {
                    if request.id == 1 {
                        gate.lock().unwrap().recv().unwrap();
                    }
                    Ok(String::new())
                },
            )
        });
        read_packet(&mut reader);
        write!(
            client,
            "{}{}{}",
            request(1, "dashboard-data"),
            request(2, "dashboard-data"),
            request(if duplicate { 1 } else { 3 }, "network-state")
        )
        .unwrap();
        let packet = read_packet(&mut reader);
        if duplicate {
            assert_eq!(packet, value!({"fatal":"Duplicate helper request ID"}));
        } else {
            assert_eq!(
                packet,
                value!({"id":3,"code":1,"output":r#"{"error": "Helper queue is full"}"#})
            );
        }
        release.send(()).unwrap();
        assert_eq!(read_packet(&mut reader)["id"], 1);
        assert_eq!(read_packet(&mut reader)["id"], 2);
        client.shutdown(std::net::Shutdown::Write).unwrap();
        assert_eq!(worker.join().unwrap().unwrap(), i32::from(duplicate));
    }
}
#[test]
fn response_limit_returns_the_existing_error_envelope() {
    let output = Output::default();
    service::serve(
        Cursor::new(request(1, "network-state")),
        output.clone(),
        Limits {
            response: 256,
            ..Limits::default()
        },
        |_| Ok("x".repeat(1000)),
    )
    .unwrap();
    assert_eq!(
        packets(&output)[1],
        value!({"id":1,"code":1,"output":r#"{"error": "Helper response is too large"}"#})
    );
}

#[test]
fn pseudo_language_keeps_markup_and_placeholders() {
    assert_eq!(
        nacre_helper::i18n::pseudo("Hello {name} <b>World</b>"),
        "[Ħëľľö {name} <b>Ŵöŕľď</b> ~~~~]"
    );
}

#[test]
fn python_value_coercion_preserves_title_inputs() {
    assert_eq!(json::python_str(&value!(null)), "None");
    assert_eq!(json::python_str(&value!(true)), "True");
    assert_eq!(
        json::python_str(&value!(["A", null, true, 1e-5])),
        "['A', None, True, 1e-05]"
    );
    assert_eq!(
        json::python_str(&value!({"label":"λ", "hidden":"\u{200b}"})),
        "{'label': 'λ', 'hidden': '\\u200b'}"
    );
    assert_eq!(
        json::python_str(&value!(["it's", "a\u{a0}b"])),
        "[\"it's\", 'a\\xa0b']"
    );
}

#[test]
#[cfg(unix)]
fn detached_app_starts_an_independent_session() {
    use std::{fs, io::Write};
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("release");
    nix::unistd::mkfifo(
        &fifo,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    let mut child = nacre_helper::process::spawn_detached(
        "/bin/sh",
        &[
            "-c".into(),
            "read value < \"$1\"".into(),
            "fixture".into(),
            fifo.to_string_lossy().into_owned(),
        ],
    )
    .unwrap();
    let pid = child.id();
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields: Vec<_> = stat[stat.rfind(')').unwrap() + 2..]
        .split_whitespace()
        .collect();
    let independent = fields[2] == pid.to_string() && fields[3] == pid.to_string();
    fs::OpenOptions::new()
        .write(true)
        .open(fifo)
        .unwrap()
        .write_all(b"done\n")
        .unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(independent);
}

#[test]
#[cfg(unix)]
fn command_children_do_not_inherit_selector_signal_blocks() {
    use nix::sys::signal::{pthread_sigmask, SigSet, SigmaskHow, Signal};
    std::thread::spawn(|| {
        let mut signals=SigSet::empty();signals.add(Signal::SIGINT);signals.add(Signal::SIGTERM);
        let mut previous=SigSet::empty();pthread_sigmask(SigmaskHow::SIG_BLOCK,Some(&signals),Some(&mut previous)).unwrap();
        let output=nacre_helper::process::run("/bin/sh",&["-c".into(),"while IFS= read -r line; do case $line in SigBlk:*) printf '%s\\n' \"$line\"; break;; esac; done < /proc/self/status".into()],None,std::time::Duration::from_secs(3)).unwrap();
        pthread_sigmask(SigmaskHow::SIG_SETMASK,Some(&previous),None).unwrap();
        let text=String::from_utf8(output.stdout).unwrap();
        let mask=u64::from_str_radix(text.split_whitespace().nth(1).unwrap(),16).unwrap();
        assert_eq!(mask & ((1<<(Signal::SIGINT as u32-1))|(1<<(Signal::SIGTERM as u32-1))),0);
    }).join().unwrap();
}
