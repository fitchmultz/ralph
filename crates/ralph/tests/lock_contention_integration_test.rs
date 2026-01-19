use anyhow::{Context, Result};
use ralph::{fsutil, queue};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn current_exe() -> PathBuf {
    std::env::current_exe().expect("resolve current test executable path")
}

#[test]
fn lock_holder_process() -> Result<()> {
    if std::env::var("RALPH_TEST_LOCK_HOLD").ok().as_deref() != Some("1") {
        return Ok(());
    }

    let repo_root = std::env::var("RALPH_TEST_REPO_ROOT").context("read RALPH_TEST_REPO_ROOT")?;
    let repo_root = PathBuf::from(repo_root);

    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let _lock = queue::acquire_queue_lock(&repo_root, "lock holder", false)?;
    println!("LOCK_HELD");
    let _ = std::io::stdout().flush();

    thread::sleep(Duration::from_secs(30));
    Ok(())
}

#[test]
fn lock_contention_blocks_second_process() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let mut child = Command::new(current_exe())
        .arg("--exact")
        .arg("lock_holder_process")
        .arg("--nocapture")
        .env("RALPH_TEST_LOCK_HOLD", "1")
        .env("RALPH_TEST_REPO_ROOT", &repo_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn lock holder process")?;

    let stdout = child.stdout.take().context("capture lock holder stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut got_signal = false;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if line.contains("LOCK_HELD") {
                    got_signal = true;
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }

    anyhow::ensure!(got_signal, "lock holder did not signal readiness");

    let err = queue::acquire_queue_lock(&repo_root, "contender", false).unwrap_err();
    let msg = format!("{err:#}");
    let lock_dir = fsutil::queue_lock_dir(&repo_root);

    anyhow::ensure!(
        msg.contains(lock_dir.to_string_lossy().as_ref()),
        "expected lock path in error: {msg}"
    );

    let _ = child.kill();
    let _ = child.wait();

    let _ = std::fs::remove_dir_all(&lock_dir);

    Ok(())
}
