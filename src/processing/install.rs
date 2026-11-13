//! Installation et désinstallation du moteur et des modèles, en arrière-plan.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{
    hide_console, InstallLocation, ModelSpec, TorchVariant, AUDIO_SEPARATOR_VERSION, CATALOG,
    GET_PIP_URL, PYTHON_PACKAGES, PYTHON_URL,
};

/// Ce qu'une tâche est en train de faire, pour la barre de progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Starting,
    Python,
    Pip,
    TorchDownload,
    Packages,
    Check,
    Model,
    Removing,
}

impl Step {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Starting => "processing.step.starting",
            Self::Python => "processing.step.python",
            Self::Pip => "processing.step.pip",
            Self::TorchDownload => "processing.step.torch",
            Self::Packages => "processing.step.packages",
            Self::Check => "processing.step.check",
            Self::Model => "processing.step.model",
            Self::Removing => "processing.step.removing",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Running,
    Done,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    InstallRuntime,
    DownloadModel(&'static str),
    Remove,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskSnapshot {
    pub step: Step,
    /// Octets reçus et attendus d'un téléchargement en cours.
    pub bytes: Option<(u64, u64)>,
    /// Dernière ligne utile du programme d'installation.
    pub detail: String,
    pub outcome: Outcome,
}

struct Shared {
    snapshot: Mutex<TaskSnapshot>,
    cancel: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Shared {
    fn set_step(&self, step: Step) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.step = step;
            snapshot.bytes = None;
            snapshot.detail.clear();
        }
    }

    fn set_bytes(&self, done: u64, total: u64) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.bytes = Some((done, total));
        }
    }

    fn set_detail(&self, detail: &str) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.detail = detail.chars().take(160).collect();
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

pub struct TaskHandle {
    pub kind: TaskKind,
    shared: Arc<Shared>,
}

impl TaskHandle {
    fn spawn(
        kind: TaskKind,
        work: impl FnOnce(&Shared) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(TaskSnapshot {
                step: Step::Starting,
                bytes: None,
                detail: String::new(),
                outcome: Outcome::Running,
            }),
            cancel: AtomicBool::new(false),
            child: Mutex::new(None),
        });
        let worker = shared.clone();
        std::thread::spawn(move || {
            let result = work(&worker);
            let outcome = match result {
                _ if worker.cancelled() => Outcome::Cancelled,
                Ok(()) => Outcome::Done,
                Err(error) => Outcome::Failed(error),
            };
            if let Ok(mut snapshot) = worker.snapshot.lock() {
                snapshot.outcome = outcome;
            }
        });
        Self { kind, shared }
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        self.shared
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or(TaskSnapshot {
                step: Step::Starting,
                bytes: None,
                detail: String::new(),
                outcome: Outcome::Failed("task state unavailable".into()),
            })
    }

    /// Arrête la tâche : le téléchargement s'interrompt au prochain bloc et le
    /// programme en cours (pip) est tué. Les fichiers partiels restent pour
    /// reprendre plus tard.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.shared.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

pub fn install_runtime(location: InstallLocation, variant: TorchVariant) -> TaskHandle {
    TaskHandle::spawn(TaskKind::InstallRuntime, move |shared| {
        install_runtime_steps(shared, &location, variant)
    })
}

pub fn download_model(location: InstallLocation, model: &'static ModelSpec) -> TaskHandle {
    TaskHandle::spawn(TaskKind::DownloadModel(model.id), move |shared| {
        require_writable(&location)?;
        shared.set_step(Step::Model);
        fs::create_dir_all(location.models_dir()).map_err(|error| error.to_string())?;
        download(shared, model.config_url, &location.config_path(model), None)?;
        download(
            shared,
            model.checkpoint_url,
            &location.checkpoint_path(model),
            Some(model.checkpoint_bytes),
        )
    })
}

/// Supprime des fichiers ou des dossiers, puis `empty_folder` s'il ne reste
/// rien dedans. Plusieurs gigaoctets : hors du fil de l'interface.
pub fn remove_paths(paths: Vec<PathBuf>, empty_folder: Option<PathBuf>) -> TaskHandle {
    TaskHandle::spawn(TaskKind::Remove, move |shared| {
        shared.set_step(Step::Removing);
        let mut first_error = None;
        for path in paths {
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else if path.exists() {
                fs::remove_file(&path)
            } else {
                Ok(())
            };
            if let Err(error) = result {
                first_error.get_or_insert(format!("{} : {error}", path.display()));
            }
        }
        if let Some(folder) = empty_folder {
            // `remove_dir` refuse un dossier non vide : ce que l'utilisateur
            // y aurait rangé lui-même reste en place.
            let _ = fs::remove_dir(folder);
        }
        first_error.map_or(Ok(()), Err)
    })
}

/// Fichiers d'un modèle à effacer pour le désinstaller.
pub fn model_paths(location: &InstallLocation, model: &ModelSpec) -> Vec<PathBuf> {
    let checkpoint = location.checkpoint_path(model);
    let mut paths = vec![partial_path(&checkpoint), checkpoint];
    // Une configuration peut servir à plusieurs poids : elle ne part qu'avec
    // le dernier.
    let shared_config = CATALOG.iter().any(|other| {
        other.id != model.id && other.config_file == model.config_file && location.has_model(other)
    });
    if !shared_config {
        paths.push(location.config_path(model));
    }
    paths
}

fn require_writable(location: &InstallLocation) -> Result<(), String> {
    if location.is_writable() {
        Ok(())
    } else {
        Err(format!(
            "{} : write access denied",
            location.root().display()
        ))
    }
}

fn install_runtime_steps(
    shared: &Shared,
    location: &InstallLocation,
    variant: TorchVariant,
) -> Result<(), String> {
    if !super::platform_supported() {
        return Err("unsupported platform".into());
    }
    require_writable(location)?;
    let python = location.python_dir();
    let downloads = location.downloads_dir();
    fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
    // Un marqueur périmé ne doit pas faire croire que le moteur est prêt.
    let _ = fs::remove_file(location.runtime_marker());

    shared.set_step(Step::Python);
    if !location.python_exe().is_file() {
        let archive = downloads.join("python-embed.zip");
        download(shared, PYTHON_URL, &archive, None)?;
        let _ = fs::remove_dir_all(&python);
        extract_zip(&archive, &python)?;
        let _ = fs::remove_file(&archive);
    }
    enable_site_packages(&python)?;
    check_cancel(shared)?;

    shared.set_step(Step::Pip);
    if !python
        .join("Lib")
        .join("site-packages")
        .join("pip")
        .is_dir()
    {
        let get_pip = downloads.join("get-pip.py");
        download(shared, GET_PIP_URL, &get_pip, None)?;
        let mut command = python_command(location);
        command
            .arg(&get_pip)
            .args(["--no-warn-script-location", "--disable-pip-version-check"]);
        run(shared, command)?;
        let _ = fs::remove_file(&get_pip);
    }
    check_cancel(shared)?;

    shared.set_step(Step::TorchDownload);
    let wheel = downloads.join(variant.wheel_file());
    download(shared, &variant.wheel_url(), &wheel, None)?;
    check_cancel(shared)?;

    shared.set_step(Step::Packages);
    let mut command = pip_command(location);
    command.arg(&wheel).args(PYTHON_PACKAGES);
    run(shared, command)?;
    let mut command = pip_command(location);
    command
        .arg("--no-deps")
        .arg(format!("audio-separator=={AUDIO_SEPARATOR_VERSION}"));
    run(shared, command)?;
    let _ = fs::remove_file(&wheel);
    check_cancel(shared)?;

    shared.set_step(Step::Check);
    let script = write_engine_script(location)?;
    let mut command = python_command(location);
    command.arg(&script).arg("probe");
    run(shared, command)?;
    location
        .write_runtime_marker(variant)
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&downloads);
    Ok(())
}

fn check_cancel(shared: &Shared) -> Result<(), String> {
    if shared.cancelled() {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

/// Python portable, sans console, isolé des Python de la machine.
pub fn python_command(location: &InstallLocation) -> Command {
    let mut command = Command::new(location.python_exe());
    command
        .args(["-I", "-u"])
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .current_dir(location.python_dir())
        .stdin(Stdio::null());
    hide_console(&mut command);
    command
}

fn pip_command(location: &InstallLocation) -> Command {
    let mut command = python_command(location);
    command.args([
        "-m",
        "pip",
        "install",
        "--no-input",
        "--no-warn-script-location",
        "--disable-pip-version-check",
        "--progress-bar",
        "off",
    ]);
    command
}

/// Écrit le script du moteur à côté de Python. Réécrit à chaque lancement :
/// une mise à jour de l'application met le script à jour sans réinstaller.
pub fn write_engine_script(location: &InstallLocation) -> Result<PathBuf, String> {
    let path = location.engine_script();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&path, include_str!("separate.py")).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Le Python portable ignore `site-packages` tant que son fichier `._pth` ne
/// le lui dit pas.
fn enable_site_packages(python: &Path) -> Result<(), String> {
    let entries = fs::read_dir(python).map_err(|error| error.to_string())?;
    let pth = entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("python") && name.ends_with("._pth"))
        })
        .ok_or_else(|| "python._pth not found".to_string())?;
    let archive = pth
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace("._pth", ".zip"))
        .unwrap_or_else(|| "python311.zip".into());
    fs::write(&pth, site_packages_pth(&archive)).map_err(|error| error.to_string())
}

fn site_packages_pth(archive: &str) -> String {
    format!("{archive}\r\n.\r\nLib\\site-packages\r\nimport site\r\n")
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        // `enclosed_name` refuse les chemins qui sortiraient du dossier.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = fs::File::create(&target).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    destination.with_file_name(name)
}

/// Télécharge `url` vers `destination`. Le fichier arrive d'abord dans un
/// `.part`, repris là où il s'était arrêté si le serveur le permet.
fn download(
    shared: &Shared,
    url: &str,
    destination: &Path,
    expected_bytes: Option<u64>,
) -> Result<(), String> {
    if let Some(expected) = expected_bytes {
        if fs::metadata(destination).is_ok_and(|meta| meta.len() == expected) {
            return Ok(());
        }
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let partial = partial_path(destination);
    let mut already = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
    if expected_bytes.is_some_and(|expected| already > expected) {
        let _ = fs::remove_file(&partial);
        already = 0;
    }

    if expected_bytes != Some(already) || already == 0 {
        let open = |from: u64| {
            let mut request = ureq::get(url).header("User-Agent", "coquerythmo");
            if from > 0 {
                request = request.header("Range", format!("bytes={from}-"));
            }
            request.call()
        };
        let mut response = match open(already) {
            Ok(response) => response,
            // Un `.part` que le serveur refuse de reprendre (déjà complet ou
            // périmé) : on repart du début.
            Err(_) if already > 0 => {
                let _ = fs::remove_file(&partial);
                already = 0;
                open(0).map_err(|error| format!("{}: {error}", file_label(destination)))?
            }
            Err(error) => return Err(format!("{}: {error}", file_label(destination))),
        };
        let resumed = response.status().as_u16() == 206;
        if !resumed {
            already = 0;
        }
        let total = response
            .body()
            .content_length()
            .map(|length| length + already)
            .or(expected_bytes)
            .unwrap_or(0);
        let mut output = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&partial)
            .map_err(|error| error.to_string())?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = vec![0_u8; 1 << 18];
        let mut done = already;
        shared.set_bytes(done, total);
        loop {
            check_cancel(shared)?;
            let read = reader
                .read(&mut buffer)
                .map_err(|error| format!("{}: {error}", file_label(destination)))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            done += read as u64;
            shared.set_bytes(done, total.max(done));
        }
        output.flush().map_err(|error| error.to_string())?;
    }

    let received = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
    if expected_bytes.is_some_and(|expected| expected != received) {
        // Une taille inattendue ne se reprend pas : on repartira de zéro.
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "{}: {received} bytes received, {} expected",
            file_label(destination),
            expected_bytes.unwrap_or(0)
        ));
    }
    let _ = fs::remove_file(destination);
    fs::rename(&partial, destination).map_err(|error| error.to_string())
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Lance un programme, montre sa dernière ligne et attend sa fin. En cas
/// d'échec, l'erreur porte la fin de sa sortie.
fn run(shared: &Shared, mut command: Command) -> Result<(), String> {
    check_cancel(shared)?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Ok(mut slot) = shared.child.lock() {
        *slot = Some(child);
    }
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let error_tail = tail.clone();
    let error_reader = std::thread::spawn(move || {
        if let Some(stderr) = stderr {
            for_each_line(stderr, |line| push_tail(&error_tail, line.to_owned()));
        }
    });
    if let Some(stdout) = stdout {
        for_each_line(stdout, |line| {
            shared.set_detail(line);
            push_tail(&tail, line.to_owned());
        });
    }
    let _ = error_reader.join();
    let status = shared
        .child
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .map(|mut child| child.wait());
    check_cancel(shared)?;
    match status {
        Some(Ok(status)) if status.success() => Ok(()),
        Some(Ok(status)) => Err(tail
            .lock()
            .ok()
            .map(|lines| lines.join("\n"))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| status.to_string())),
        Some(Err(error)) => Err(error.to_string()),
        None => Err("process state unavailable".into()),
    }
}

/// Lit une sortie ligne par ligne sans jamais s'arrêter sur un octet qui
/// n'est pas de l'UTF-8 : un tuyau qu'on cesse de lire bloque le programme.
pub(crate) fn for_each_line(stream: impl Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&bytes);
                let line = line.trim();
                if !line.is_empty() {
                    each(line);
                }
            }
        }
    }
}

fn push_tail(tail: &Mutex<Vec<String>>, line: String) {
    if let Ok(mut lines) = tail.lock() {
        if lines.len() == 12 {
            lines.remove(0);
        }
        lines.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_python_is_told_to_load_site_packages() {
        let pth = site_packages_pth("python311.zip");
        assert!(pth.starts_with("python311.zip\r\n"));
        assert!(pth.contains("Lib\\site-packages"));
        assert!(pth.lines().any(|line| line == "import site"));
    }

    #[test]
    fn partial_download_sits_next_to_its_destination() {
        assert_eq!(
            partial_path(Path::new("models/Inst_GaboxFv7z.ckpt")),
            PathBuf::from("models/Inst_GaboxFv7z.ckpt.part")
        );
    }

    #[test]
    fn removing_a_model_targets_its_own_files_only() {
        let location = InstallLocation::from_chosen(Path::new(r"D:\IA"));
        let model = &CATALOG[0];
        let paths = model_paths(&location, model);
        assert!(paths.contains(&location.checkpoint_path(model)));
        assert!(paths.contains(&partial_path(&location.checkpoint_path(model))));
        assert!(paths
            .iter()
            .all(|path| path.starts_with(location.models_dir())));
    }

    #[test]
    fn uninstalling_everything_spares_what_the_user_put_in_the_folder() {
        let folder =
            std::env::temp_dir().join(format!("coquerythmo-uninstall-test-{}", std::process::id()));
        let location = InstallLocation::from_chosen(&folder);
        fs::create_dir_all(location.models_dir()).unwrap();
        fs::write(location.models_dir().join("model.ckpt"), b"weights").unwrap();
        fs::write(location.root().join("notes.txt"), b"mine").unwrap();

        let handle = remove_paths(
            location.owned_entries(),
            Some(location.root().to_path_buf()),
        );
        wait(&handle);
        assert!(!location.models_dir().exists());
        // Le fichier de l'utilisateur est toujours là, donc le dossier aussi.
        assert!(location.root().join("notes.txt").is_file());

        fs::remove_file(location.root().join("notes.txt")).unwrap();
        let handle = remove_paths(Vec::new(), Some(location.root().to_path_buf()));
        wait(&handle);
        assert!(!location.root().exists());
        fs::remove_dir_all(folder).unwrap();
    }

    fn wait(handle: &TaskHandle) {
        let started = std::time::Instant::now();
        while handle.snapshot().outcome == Outcome::Running {
            assert!(started.elapsed().as_secs() < 10, "task did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(handle.snapshot().outcome, Outcome::Done);
    }

    #[test]
    fn process_output_survives_invalid_utf8() {
        let mut lines = Vec::new();
        for_each_line(&b"ok\n\xff\xfe bad\n\n last"[..], |line| {
            lines.push(line.to_owned())
        });
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "ok");
        assert_eq!(lines[2], "last");
    }

    #[test]
    fn removing_missing_paths_succeeds() {
        let handle = remove_paths(
            vec![std::env::temp_dir().join("coquerythmo-absent-dir-test")],
            None,
        );
        wait(&handle);
    }
}
