//! Fakes only: a `parec` script that writes silent and half-scale windows.
use super::*;
use crate::daemon::hub::Hub;
use std::os::unix::fs::PermissionsExt;
use tokio::task::LocalSet;

fn fake_parec(dir: &std::path::Path, body: &str) -> OsString {
    let path = dir.join("parec");
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.into()
}
fn level(hub: &Shared) -> (bool, f64) {
    let data = hub.borrow().snapshot(TOPIC).unwrap();
    (
        data["health"]["available"] == true,
        data["level"].as_f64().unwrap(),
    )
}
async fn until(hub: &Shared, wanted: (bool, f64)) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while level(hub) != wanted {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("level stayed {:?}, wanted {wanted:?}", level(hub)));
}

#[test]
fn peak_is_the_loudest_sample_in_hundredths() {
    let samples = |values: &[i16]| {
        values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    };
    assert_eq!(peak(&samples(&[0, 0])), 0.0);
    assert_eq!(peak(&samples(&[100, -16384, 5])), 0.5);
    assert_eq!(peak(&samples(&[i16::MIN])), 1.0);
    assert_eq!(peak(&samples(&[i16::MAX])), 1.0);
    assert_eq!(peak(&[]), 0.0);
}

#[tokio::test(flavor = "current_thread")]
async fn captures_only_while_leased_and_stops_when_released() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().display();
    // Records its arguments and its pid, writes one silent then endless half-scale windows.
    let parec = fake_parec(
        dir.path(),
        &format!(
            "echo \"$@\" > '{d}/args'\necho $$ > '{d}/pid'\nhead -c 1056 /dev/zero\nwhile :; do printf '\\000\\100%.0s' $(seq 1 528); sleep 0.05; done\n"
        ),
    );
    let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec,
                    retry: Duration::from_millis(30),
                },
            ));
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!dir.path().join("pid").exists(), "no lease, no capture");
            assert_eq!(level(&hub), (false, 0.0));
            let (client, _) = hub.borrow_mut().add_client().unwrap();
            hub.borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            until(&hub, (true, 0.5)).await;
            let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
            assert!(
                args.contains("--device=@DEFAULT_SOURCE@")
                    && args.contains("--client-name=nacre-peak")
            );
            let pid: i32 = std::fs::read_to_string(dir.path().join("pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            hub.borrow_mut().remove_client(client);
            until(&hub, (false, 0.0)).await;
            // The capture child ends with the lease (kill_on_drop reaps the shell).
            tokio::time::timeout(Duration::from_secs(5), async {
                while std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("the capture process outlived its lease");
            task.abort();
            let _ = task.await;
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_or_missing_capture_is_unavailable_and_retried() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().display();
    let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    LocalSet::new()
        .run_until(async {
            let parec = fake_parec(dir.path(), &format!("echo x >> '{d}/starts'\nexit 1\n"));
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec,
                    retry: Duration::from_millis(30),
                },
            ));
            let (client, _) = hub.borrow_mut().add_client().unwrap();
            hub.borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while std::fs::read_to_string(dir.path().join("starts"))
                    .map_or(0, |s| s.lines().count())
                    < 3
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the capture was not retried");
            assert_eq!(level(&hub), (false, 0.0));
            task.abort();
            let _ = task.await;
            // A missing tool never fails the service.
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec: dir.path().join("missing").into(),
                    retry: Duration::from_millis(30),
                },
            ));
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert!(!task.is_finished());
            assert_eq!(level(&hub), (false, 0.0));
            task.abort();
            let _ = task.await;
        })
        .await;
}
