#[cfg(target_os = "linux")]
mod linux {
    use perveance::{workspace, Project};
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixListener;

    #[test]
    fn stages_only_validated_project_data_and_removes_it() {
        let parent = tempfile::tempdir().unwrap();
        let host_secret = parent.path().join("host-secret");
        fs::write(&host_secret, b"host-only-sentinel").unwrap();
        symlink(&host_secret, parent.path().join("outside-link")).unwrap();
        fs::hard_link(&host_secret, parent.path().join("outside-hardlink")).unwrap();
        let _listener = UnixListener::bind(parent.path().join("host.sock")).unwrap();
        let mut p = Project::new(2, 2).unwrap();
        p.set_pixel(0, 0, 0, [9, 8, 7, 0]).unwrap();
        p.add_layer().unwrap();
        p.rename_layer(1, "../../host-secret").unwrap(); // data, not a path
        p.layers[1].visible = false;
        p.set_pixel(1, 1, 1, [10, 20, 30, 255]).unwrap();
        let revision = p.revision().unwrap();
        let staged = workspace::stage_project(&p, parent.path()).unwrap();
        let project_path = staged.project_path();
        let dir = project_path.parent().unwrap().to_owned();
        assert_eq!(staged.revision(), revision);
        assert_eq!(project_path.file_name().unwrap(), workspace::STAGED_PROJECT_NAME);
        let entries = fs::read_dir(&dir).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), workspace::STAGED_PROJECT_NAME);
        let dir_metadata = fs::symlink_metadata(&dir).unwrap();
        let file_metadata = fs::symlink_metadata(&project_path).unwrap();
        assert!(dir_metadata.file_type().is_dir());
        assert_eq!(dir_metadata.permissions().mode() & 0o077, 0);
        assert!(file_metadata.file_type().is_file());
        assert_eq!(file_metadata.permissions().mode() & 0o077, 0);
        assert_eq!(file_metadata.nlink(), 1);
        assert_eq!(Project::load(&project_path).unwrap(), p);
        assert_eq!(fs::read(&host_secret).unwrap(), b"host-only-sentinel");
        staged.close().unwrap();
        assert!(!dir.exists());
        assert_eq!(fs::read(&host_secret).unwrap(), b"host-only-sentinel");
        assert!(parent.path().join("host.sock").exists());
    }

    #[test]
    fn staging_failure_and_drop_do_not_leave_normal_temporary_directories() {
        let parent = tempfile::tempdir().unwrap();
        let mut invalid = Project::new(1, 1).unwrap();
        invalid.layers[0].pixels.clear();
        assert!(workspace::stage_project(&invalid, parent.path()).is_err());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
        let p = Project::new(1, 1).unwrap();
        assert!(workspace::stage_project(&p, &parent.path().join("missing")).is_err());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
        let staged = workspace::stage_project(&p, parent.path()).unwrap();
        let dir = staged.project_path().parent().unwrap().to_owned();
        assert!(dir.exists());
        drop(staged);
        assert!(!dir.exists());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    /// Explicitly opt in only on a compatible test host. Fixed code and
    /// disposable fixtures only; this is not an untrusted-worker launcher.
    #[test]
    #[ignore = "requires a compatible unprivileged bubblewrap host"]
    fn fixed_staged_project_worker() {
        use std::process::{Command, Stdio};
        use std::net::{TcpListener, TcpStream};
        use std::os::unix::net::UnixStream;
        let parent = tempfile::tempdir().unwrap();
        let secret = parent.path().join("host-sentinel");
        fs::write(&secret, b"host-only-sentinel").unwrap();
        let socket_path = parent.path().join("host.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let unix_control = UnixStream::connect(&socket_path).unwrap();
        let (accepted, _) = listener.accept().unwrap();
        drop((unix_control, accepted));
        let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = tcp_listener.local_addr().unwrap().port();
        let tcp_control = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (accepted, _) = tcp_listener.accept().unwrap();
        drop((tcp_control, accepted));
        symlink(&secret, parent.path().join("outside-link")).unwrap();
        let mut project = Project::new(2, 2).unwrap();
        project.set_pixel(0, 0, 0, [9, 8, 7, 0]).unwrap();
        project.rename_layer(0, "hidden test layer").unwrap();
        project.layers[0].visible = false;
        let staged = workspace::stage_project(&project, parent.path()).unwrap();
        let stage_dir = staged.project_path().parent().unwrap().to_owned();
        let worker = r#"
import json, os, pathlib, socket, sys
p = json.loads(pathlib.Path('/work/project.perveance.json').read_text(encoding='utf-8'))
assert p['width'] == 2 and p['height'] == 2
assert p['layers'][0]['name'] == 'hidden test layer'
assert p['layers'][0]['visible'] is False and p['layers'][0]['pixels'][0] == [9,8,7,0]
assert os.listdir('/work') == ['project.perveance.json']
assert not pathlib.Path('/work/outside-link').exists()
try:
    pathlib.Path(sys.argv[1]).read_bytes()
except OSError:
    pass
else:
    raise AssertionError('host sentinel was readable')
try:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(1)
        s.connect(sys.argv[2])
except OSError:
    pass
else:
    raise AssertionError('host socket was reachable')
try:
    with socket.create_connection(('127.0.0.1', int(sys.argv[3])), timeout=1):
        raise AssertionError('host loopback was reachable')
except OSError:
    pass
assert 'PERVEANCE_PROBE_SECRET' not in os.environ
pathlib.Path('/work/fixed-output').write_bytes(b'fixed-output')
print('fixed staged project checks passed')
"#;
        let mut command = Command::new("/usr/bin/bwrap");
        command.args(["--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--new-session", "--die-with-parent", "--clearenv", "--setenv", "PATH", "/usr/bin:/bin"]);
        for path in ["/usr", "/bin", "/lib", "/lib64"] {
            if std::path::Path::new(path).exists() { command.args(["--ro-bind", path, path]); }
        }
        command.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--bind"])
            .arg(&stage_dir).args(["/work", "--chdir", "/work", "/usr/bin/python3", "-I", "-c", worker])
            .arg(&secret).arg(&socket_path).arg(port.to_string())
            .env_clear().env("PATH", "/usr/bin:/bin").env("PERVEANCE_PROBE_SECRET", "synthetic")
            .stdin(Stdio::null());
        let result = command.output().expect("start fixed bubblewrap test");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(result.stdout, b"fixed staged project checks passed\n");
        assert_eq!(fs::read(stage_dir.join("fixed-output")).unwrap(), b"fixed-output");
        assert_eq!(fs::read(&secret).unwrap(), b"host-only-sentinel");
        staged.close().unwrap();
        assert!(!stage_dir.exists());
        assert!(socket_path.exists());
    }
}
