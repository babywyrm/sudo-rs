use pretty_assertions::assert_eq;
use sudo_test::{Command, Env};

use crate::{
    Result, SUDOERS_ALL_ALL_NOPASSWD, SUDOERS_USER_ALL_NOPASSWD, SUDOERS_USE_PTY, USERNAME,
};

// man sudo > Signal handling
// "As a special case, sudo will not relay signals that were sent by the command it is running."
#[test]
fn signal_sent_by_child_process_is_ignored() -> Result<()> {
    let script = include_str!("kill-sudo-parent.sh");

    let kill_sudo_parent = "/root/kill-sudo-parent.sh";
    let env = Env([SUDOERS_USER_ALL_NOPASSWD, SUDOERS_USE_PTY])
        .user(USERNAME)
        .file(kill_sudo_parent, script)
        .build()?;

    Command::new("sudo")
        .args(["sh", kill_sudo_parent])
        .as_user(USERNAME)
        .tty(true)
        .exec(&env)?
        .assert_success()
}

#[test]
fn signal_is_forwarded_to_child() -> Result<()> {
    let expected = "got signal";
    let expects_signal = "/root/expects-signal.sh";
    let kill_sudo = "/root/kill-sudo.sh";
    let env = Env([SUDOERS_USER_ALL_NOPASSWD, SUDOERS_USE_PTY])
        .user(USERNAME)
        .file(expects_signal, include_str!("expects-signal.sh"))
        .file(kill_sudo, include_str!("kill-sudo.sh"))
        .build()?;

    let child = Command::new("sudo")
        .args(["sh", expects_signal])
        .as_user(USERNAME)
        .spawn(&env)?;

    Command::new("sh")
        .arg(kill_sudo)
        .tty(true)
        .exec(&env)?
        .assert_success()?;

    let actual = child.wait()?.stdout()?;

    assert_eq!(expected, actual);

    Ok(())
}

// man sudo > Exit value
// "If the command terminated due to receipt of a signal, sudo will send itself the same signal that terminated the command."
#[test]
fn child_terminated_by_signal() -> Result<()> {
    let env = Env([SUDOERS_USER_ALL_NOPASSWD, SUDOERS_USE_PTY])
        .user(USERNAME)
        .build()?;

    // child process sends SIGTERM to itself
    let output = Command::new("sudo")
        .args(["sh", "-c", "kill $$"])
        .as_user(USERNAME)
        .tty(true)
        .exec(&env)?;

    assert_eq!(Some(143), output.status().code());
    assert!(output.stderr().is_empty());

    Ok(())
}

#[test]
#[ignore = "gh325"]
fn sigtstp_works() -> Result<()> {
    const STOP_DELAY: u64 = 5;
    const NUM_ITERATIONS: usize = 5;

    let script_path = "/tmp/script.sh";
    let env = Env([SUDOERS_ALL_ALL_NOPASSWD, SUDOERS_USE_PTY])
        .file(script_path, include_str!("sigtstp.bash"))
        .build()?;

    let output = Command::new("bash")
        .arg(script_path)
        .tty(true)
        .exec(&env)?
        .stdout()?;

    let timestamps = output
        .lines()
        .filter_map(|line| {
            // when testing the use_pty-enabled ogsudo we have observed a `\r\r\n` line ending,
            // instead of the regular `\r\n` line ending that the `lines` adapter will remove. use
            // `trim_end` to remove the `\r` that `lines` won't remove
            line.trim_end().parse::<u64>().ok()
        })
        .collect::<Vec<_>>();

    dbg!(&timestamps);

    assert_eq!(NUM_ITERATIONS, timestamps.len());

    let suspended_iterations = timestamps
        .windows(2)
        .filter(|window| {
            let prev_timestamp = window[0];
            let curr_timestamp = window[1];
            let delta = curr_timestamp - prev_timestamp;

            delta >= STOP_DELAY
        })
        .count();
    let did_suspend = suspended_iterations == 1;

    assert!(did_suspend);

    Ok(())
}
