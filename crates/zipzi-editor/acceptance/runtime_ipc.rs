//! Installed public Python clients publish into the actual native Editor view.
use super::harness::{Harness, update};
use anyhow::{Context, ensure};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use zipzi_editor_core::{Editor, EditorCommand};

fn client(
    h: &mut Harness,
    python: &Path,
    endpoint: &Path,
    script: &str,
    args: &[String],
) -> anyhow::Result<()> {
    let mut child = Command::new(python)
        .args(["-I", "-c", script])
        .args(args)
        .env_remove("PYTHONPATH")
        .env("ZIPZI_RUNTIME_DIR", endpoint)
        .env("ZIPZI_RUNTIME_PROFILE", "test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait(h, &mut child)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Python client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
fn wait(h: &mut Harness, child: &mut Child) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        h.cx.advance_clock(Duration::from_millis(2));
        h.cx.run_until_parked();
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            anyhow::bail!("Python client did not finish");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub(super) fn run() -> anyhow::Result<()> {
    let python = std::env::var_os("ZIPZI_CLIENT_PYTHON")
        .context("ZIPZI_CLIENT_PYTHON must point to an isolated installed client")?;
    let python = Path::new(&python);
    ensure!(python.is_absolute(), "client interpreter must be absolute");
    let endpoint = tempfile::Builder::new()
        .prefix("zzipc-")
        .tempdir_in("/tmp")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(endpoint.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut h = Harness::new()?;
    let host = crate::host::DesktopHost::new();
    let (document, _assets) = super::fixture::sample_document()?;
    let runtime = host.runtime.clone();
    h.cx.update(|cx| {
        cx.set_global(host.clone());
        let owner = zipzi_runtime::local::Endpoint::new(endpoint.path(), "test")?
            .claim()?
            .context("endpoint occupied")?;
        crate::ipc::start(owner, cx)?;
        Ok::<_, anyhow::Error>(())
    })?;
    update(&mut h, |view, window, cx| {
        view.host = Some(host.clone());
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    let (session, mesh, before, revision) = h.view.read_with(&h.cx, |view, _| {
        let document = view.editor.document().unwrap();
        (
            document.runtime_session_id(),
            view.editor.layers()[0].id.clone(),
            document.history_lengths(),
            document.properties_revision(),
        )
    });
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
from importlib.metadata import version
from importlib.util import find_spec
assert version('zipzi-python2')=='0.1.0'
assert '/site-packages/zipzi/' in zipzi.__file__
assert find_spec('zipzi_native') is None
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.id in {info.id for info in c.sessions.list()}
 with s.edit('external Python') as e:
  e.meshes.rename(sys.argv[3], 'Python changed this mesh')
 assert s.meshes.get(sys.argv[3]).name=='Python changed this mesh'
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
        ],
    )?;
    h.cx.run_until_parked();
    h.view.read_with(&h.cx, |view, _| {
        let document = view
            .editor
            .document()
            .context("external job left view busy")?;
        ensure!(
            document.properties_revision() > revision,
            "properties projection did not advance"
        );
        ensure!(
            view.editor
                .layers()
                .iter()
                .any(|l| l.id == mesh && l.name == "Python changed this mesh"),
            "layer projection not updated"
        );
        ensure!(
            document.history_lengths() == (before.0 + 1, 0),
            "duplicate/missing history"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    // Human undo is immediately visible to another real Python process.
    update(&mut h, |view, _, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    })?;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.meshes.get(sys.argv[3]).name!='Python changed this mesh'
 assert s.info().history[1]==1
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
        ],
    )?;
    // UI owns an in-flight Save As; the external entry cannot bypass its target.
    let target = endpoint.path().join("project");
    let task = h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, _| {
            view.editor
                .prepare_save(view.editor.active_id().unwrap(), &target, false)
        })
    })??;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,uuid,zipzi
with zipzi.connect() as c:
 s=c.sessions.create(str(uuid.uuid4()))
 try: s.project.save(__import__('pathlib').Path(sys.argv[1])).wait()
 except zipzi.Busy: pass
 else: raise AssertionError('cross-session target reservation bypassed')
"#,
        &[target.to_string_lossy().into_owned()],
    )?;
    ensure!(
        !target.exists(),
        "rejected external save modified UI target"
    );
    let mut completion = task.run();
    completion.publish(&runtime);
    update(&mut h, |view, _, cx| {
        view.editor.finish_save(&mut completion).unwrap();
        view.redraw(cx);
    })?;
    ensure!(
        runtime.read().session_info(session)?.state == zipzi_runtime::SessionState::Ready,
        "save stayed busy"
    );
    // Render through Metal after the external publication/UI history cycle.
    let image = h.capture()?;
    ensure!(
        image.width() > 0 && image.height() > 0,
        "native render missing"
    );
    println!(
        "R3 native IPC: installed zipzi-python2 -> same unsaved session -> layer/properties/history -> UI Undo -> path reservation -> Metal render passed"
    );
    Ok(())
}
