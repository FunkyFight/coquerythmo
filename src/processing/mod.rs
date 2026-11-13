//! Onglet Traitement : retirer la voix d'un audio, isoler des instruments ou
//! nettoyer une voix avec des modèles Roformer.
//!
//! Rien n'est livré avec l'application : le moteur (un Python portable et
//! PyTorch) et chaque modèle se téléchargent à la demande dans le dossier
//! que l'utilisateur choisit, et se désinstallent de la même façon.

pub mod install;
pub mod job;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::recording::WaveformData;

/// Version de l'installation : la changer oblige à réinstaller le moteur.
pub const RUNTIME_VERSION: u32 = 1;

pub const PYTHON_VERSION: &str = "3.11.9";
pub const PYTHON_URL: &str =
    "https://www.python.org/ftp/python/3.11.9/python-3.11.9-embed-amd64.zip";
pub const GET_PIP_URL: &str = "https://bootstrap.pypa.io/get-pip.py";
pub const AUDIO_SEPARATOR_VERSION: &str = "0.48.0";

/// Paquets nécessaires aux classes Roformer, en plus de PyTorch.
pub const PYTHON_PACKAGES: &[&str] = &[
    "einops>=0.7",
    "rotary-embedding-torch>=0.6.1,<0.7.0",
    "beartype>=0.18.5,<0.19.0",
    "librosa>=0.10",
    "soundfile>=0.12",
    "pyyaml",
    "packaging",
    "numpy",
];

/// Fréquence et canaux attendus par tous les modèles du catalogue.
pub const MODEL_SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorchVariant {
    /// RTX 20 à 50 et GTX 16.
    Cuda128,
    /// Cartes NVIDIA plus anciennes (GTX 9 et 10).
    Cuda126,
    Cpu,
}

impl TorchVariant {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Cuda128 => "cu128",
            Self::Cuda126 => "cu126",
            Self::Cpu => "cpu",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "cu128" => Some(Self::Cuda128),
            "cu126" => Some(Self::Cuda126),
            "cpu" => Some(Self::Cpu),
            _ => None,
        }
    }

    pub fn wheel_file(self) -> String {
        format!("torch-2.8.0+{}-cp311-cp311-win_amd64.whl", self.tag())
    }

    pub fn wheel_url(self) -> String {
        format!(
            "https://download.pytorch.org/whl/{tag}/torch-2.8.0%2B{tag}-cp311-cp311-win_amd64.whl",
            tag = self.tag()
        )
    }

    /// Taille du téléchargement, pour l'annoncer avant de commencer.
    pub fn download_bytes(self) -> u64 {
        match self {
            Self::Cuda128 => 3_461_420_395,
            Self::Cuda126 => 2_915_474_854,
            Self::Cpu => 619_392_861,
        }
    }

    pub fn uses_gpu(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// Carte graphique NVIDIA vue par le pilote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
}

impl GpuInfo {
    pub fn torch_variant(&self) -> TorchVariant {
        if legacy_nvidia_card(&self.name) {
            TorchVariant::Cuda126
        } else {
            TorchVariant::Cuda128
        }
    }

    /// Une carte de la gamme conseillée (RTX, au moins 8 Go).
    pub fn is_recommended(&self) -> bool {
        self.name.to_ascii_uppercase().contains("RTX") && self.vram_mb >= 7_500
    }
}

fn legacy_nvidia_card(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["GTX 9", "GTX 10", "GT 10", "GT 7", "GTX 7", "TITAN X"]
        .iter()
        .any(|family| upper.contains(family))
}

/// Interroge le pilote NVIDIA. `None` : pas de carte NVIDIA utilisable.
pub fn detect_gpu() -> Option<GpuInfo> {
    let mut command = std::process::Command::new("nvidia-smi");
    command
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_gpu_line(String::from_utf8_lossy(&output.stdout).lines().next()?)
}

fn parse_gpu_line(line: &str) -> Option<GpuInfo> {
    let (name, memory) = line.rsplit_once(',')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(GpuInfo {
        name: name.to_owned(),
        vram_mb: memory.trim().parse().unwrap_or(0),
    })
}

/// Les processus lancés depuis l'interface ne doivent pas ouvrir de console.
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// Une piste produite par un modèle. `name` est le nom donné par la
/// configuration du modèle, `label_key` sa traduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StemSpec {
    pub name: &'static str,
    pub label_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub purpose_key: &'static str,
    pub checkpoint_url: &'static str,
    pub checkpoint_file: &'static str,
    pub checkpoint_bytes: u64,
    pub config_url: &'static str,
    pub config_file: &'static str,
    pub stems: &'static [StemSpec],
}

const INSTRUMENTAL: StemSpec = StemSpec {
    name: "Instrumental",
    label_key: "processing.stem.instrumental",
};
const VOCALS: StemSpec = StemSpec {
    name: "Vocals",
    label_key: "processing.stem.vocals",
};

/// Modèles proposés. Le premier est celui de l'usage principal : retirer la
/// voix. Noms de fichiers vérifiés sur les dépôts le 12/11/2026.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "inst_fv7z",
        title: "Mel-Band RoFormer Inst FV7z (Gabox)",
        purpose_key: "processing.model.inst_fv7z",
        checkpoint_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/instrumental/Inst_GaboxFv7z.ckpt",
        checkpoint_file: "Inst_GaboxFv7z.ckpt",
        checkpoint_bytes: 913_031_195,
        config_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/instrumental/inst_gabox.yaml",
        config_file: "inst_gabox.yaml",
        stems: &[INSTRUMENTAL, VOCALS],
    },
    ModelSpec {
        id: "voc_fv6",
        title: "Mel-Band RoFormer Voc FV6 (Gabox)",
        purpose_key: "processing.model.voc_fv6",
        checkpoint_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/vocals/voc_fv6.ckpt",
        checkpoint_file: "voc_fv6.ckpt",
        checkpoint_bytes: 913_031_195,
        config_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/vocals/voc_gabox.yaml",
        config_file: "voc_gabox.yaml",
        stems: &[VOCALS, INSTRUMENTAL],
    },
    ModelSpec {
        id: "splitter_sw",
        title: "BS-RoFormer SW (jarredou)",
        purpose_key: "processing.model.splitter_sw",
        checkpoint_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/BS-Roformer-SW.ckpt",
        checkpoint_file: "BS-Roformer-SW.ckpt",
        checkpoint_bytes: 699_412_152,
        config_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/BS-Roformer-SW.yaml",
        config_file: "BS-Roformer-SW.yaml",
        stems: &[
            StemSpec {
                name: "bass",
                label_key: "processing.stem.bass",
            },
            StemSpec {
                name: "drums",
                label_key: "processing.stem.drums",
            },
            StemSpec {
                name: "other",
                label_key: "processing.stem.other",
            },
            StemSpec {
                name: "vocals",
                label_key: "processing.stem.vocals",
            },
            StemSpec {
                name: "guitar",
                label_key: "processing.stem.guitar",
            },
            StemSpec {
                name: "piano",
                label_key: "processing.stem.piano",
            },
        ],
    },
    ModelSpec {
        id: "denoise",
        title: "Mel-Band RoFormer Denoise (aufr33)",
        purpose_key: "processing.model.denoise",
        checkpoint_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/denoise_mel_band_roformer_aufr33_sdr_27.9959.ckpt",
        checkpoint_file: "denoise_mel_band_roformer_aufr33_sdr_27.9959.ckpt",
        checkpoint_bytes: 913_097_300,
        config_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/denoise_mel_band_roformer_aufr33_sdr_27.9959_config.yaml",
        config_file: "denoise_mel_band_roformer_aufr33_sdr_27.9959_config.yaml",
        stems: &[
            StemSpec {
                name: "dry",
                label_key: "processing.stem.denoised",
            },
            StemSpec {
                name: "other",
                label_key: "processing.stem.noise",
            },
        ],
    },
    ModelSpec {
        id: "dereverb",
        title: "Mel-Band RoFormer De-Reverb (anvuew)",
        purpose_key: "processing.model.dereverb",
        checkpoint_url: "https://huggingface.co/anvuew/dereverb_mel_band_roformer/resolve/main/dereverb_mel_band_roformer_anvuew_sdr_19.1729.ckpt",
        checkpoint_file: "dereverb_mel_band_roformer_anvuew_sdr_19.1729.ckpt",
        checkpoint_bytes: 913_107_578,
        config_url: "https://huggingface.co/anvuew/dereverb_mel_band_roformer/resolve/main/dereverb_mel_band_roformer_anvuew.yaml",
        config_file: "dereverb_mel_band_roformer_anvuew.yaml",
        stems: &[
            StemSpec {
                name: "noreverb",
                label_key: "processing.stem.dry",
            },
            StemSpec {
                name: "reverb",
                label_key: "processing.stem.reverb",
            },
        ],
    },
    ModelSpec {
        id: "aspiration",
        title: "Mel-Band RoFormer Aspiration (Sucial)",
        purpose_key: "processing.model.aspiration",
        checkpoint_url: "https://huggingface.co/Sucial/Aspiration_Mel_Band_Roformer/resolve/main/aspiration_mel_band_roformer_sdr_18.9845.ckpt",
        checkpoint_file: "aspiration_mel_band_roformer_sdr_18.9845.ckpt",
        checkpoint_bytes: 835_983_746,
        config_url: "https://huggingface.co/Sucial/Aspiration_Mel_Band_Roformer/resolve/main/config_aspiration_mel_band_roformer.yaml",
        config_file: "config_aspiration_mel_band_roformer.yaml",
        stems: &[
            StemSpec {
                name: "aspiration",
                label_key: "processing.stem.breaths",
            },
            StemSpec {
                name: "other",
                label_key: "processing.stem.without_breaths",
            },
        ],
    },
    ModelSpec {
        id: "crowd",
        title: "Mel-Band RoFormer Crowd (aufr33, viperx)",
        purpose_key: "processing.model.crowd",
        checkpoint_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/mel_band_roformer_crowd_aufr33_viperx_sdr_8.7144.ckpt",
        checkpoint_file: "mel_band_roformer_crowd_aufr33_viperx_sdr_8.7144.ckpt",
        checkpoint_bytes: 913_096_801,
        config_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/mel_band_roformer_crowd_aufr33_viperx_sdr_8.7144_config.yaml",
        config_file: "mel_band_roformer_crowd_aufr33_viperx_sdr_8.7144_config.yaml",
        stems: &[
            StemSpec {
                name: "crowd",
                label_key: "processing.stem.crowd",
            },
            StemSpec {
                name: "other",
                label_key: "processing.stem.without_crowd",
            },
        ],
    },
    ModelSpec {
        id: "male_female",
        title: "BS-RoFormer Male/Female (aufr33)",
        purpose_key: "processing.model.male_female",
        checkpoint_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/bs_roformer_male_female_by_aufr33_sdr_7.2889.ckpt",
        checkpoint_file: "bs_roformer_male_female_by_aufr33_sdr_7.2889.ckpt",
        checkpoint_bytes: 527_119_779,
        config_url: "https://github.com/nomadkaraoke/python-audio-separator/releases/download/model-configs/config_chorus_male_female_bs_roformer.yaml",
        config_file: "config_chorus_male_female_bs_roformer.yaml",
        stems: &[
            StemSpec {
                name: "male",
                label_key: "processing.stem.male",
            },
            StemSpec {
                name: "female",
                label_key: "processing.stem.female",
            },
        ],
    },
    ModelSpec {
        id: "karaoke_small",
        title: "Mel-Band RoFormer Karaoke small (Gabox)",
        purpose_key: "processing.model.karaoke_small",
        checkpoint_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/karaoke/small_karaoke_gaboxaufr.ckpt",
        checkpoint_file: "small_karaoke_gaboxaufr.ckpt",
        checkpoint_bytes: 202_574_227,
        config_url: "https://huggingface.co/GaboxR67/MelBandRoformers/resolve/main/melbandroformers/karaoke/config_karaoke_small.yaml",
        config_file: "config_karaoke_small.yaml",
        stems: &[
            StemSpec {
                name: "Vocals",
                label_key: "processing.stem.lead_vocals",
            },
            StemSpec {
                name: "Instrumental",
                label_key: "processing.stem.backing",
            },
        ],
    },
];

impl ModelSpec {
    pub fn stem_label_key(&self, name: &str) -> Option<&'static str> {
        self.stems
            .iter()
            .find(|stem| stem.name.eq_ignore_ascii_case(name))
            .map(|stem| stem.label_key)
    }
}

fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
}

/// Nom du sous-dossier créé dans le dossier choisi. Tout ce que l'onglet
/// installe y reste : le dossier de l'utilisateur n'est jamais touché
/// au-delà.
pub const LOCATION_FOLDER: &str = "coquerythmo-processing";

/// Où vivent le moteur et les modèles : le sous-dossier
/// [`LOCATION_FOLDER`] du dossier que l'utilisateur a choisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallLocation {
    root: PathBuf,
}

impl InstallLocation {
    /// À partir du dossier désigné dans la boîte de dialogue. Redésigner un
    /// dossier `coquerythmo-processing` existant le reprend tel quel.
    pub fn from_chosen(folder: &Path) -> Self {
        let already = folder
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(LOCATION_FOLDER));
        Self {
            root: if already {
                folder.to_path_buf()
            } else {
                folder.join(LOCATION_FOLDER)
            },
        }
    }

    /// L'emplacement enregistré dans les réglages, s'il y en a un.
    pub fn configured() -> Option<Self> {
        crate::config::processing_directory().map(|root| Self { root })
    }

    /// Proposition par défaut : à côté de l'exécutable, quand on peut y
    /// écrire (copie portable, installation hors de `Program Files`).
    pub fn suggested() -> Option<Self> {
        let folder = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let location = Self::from_chosen(&folder);
        let existed = location.root.is_dir();
        let writable = location.is_writable();
        if !existed {
            // La vérification a créé le dossier : rien n'est encore décidé.
            let _ = std::fs::remove_dir(&location.root);
        }
        writable.then_some(location)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Peut-on écrire ici ? Crée le sous-dossier au passage.
    pub fn is_writable(&self) -> bool {
        if std::fs::create_dir_all(&self.root).is_err() {
            return false;
        }
        let probe = self.root.join(".write-test");
        let writable = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        writable
    }

    pub fn python_dir(&self) -> PathBuf {
        self.root.join("python")
    }

    pub fn python_exe(&self) -> PathBuf {
        self.python_dir().join("python.exe")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join("engine")
    }

    pub fn engine_script(&self) -> PathBuf {
        self.engine_dir().join("separate.py")
    }

    pub fn runtime_marker(&self) -> PathBuf {
        self.root.join("runtime.txt")
    }

    pub fn checkpoint_path(&self, model: &ModelSpec) -> PathBuf {
        self.models_dir().join(model.checkpoint_file)
    }

    pub fn config_path(&self, model: &ModelSpec) -> PathBuf {
        self.models_dir().join(model.config_file)
    }

    /// Le poids est complet (taille exacte) et sa configuration est là.
    pub fn has_model(&self, model: &ModelSpec) -> bool {
        file_len(&self.checkpoint_path(model)) == Some(model.checkpoint_bytes)
            && file_len(&self.config_path(model)).is_some_and(|len| len > 0)
    }

    /// Variante de PyTorch installée, si le moteur est complet et à jour.
    pub fn installed_runtime(&self) -> Option<TorchVariant> {
        if !self.python_exe().is_file() {
            return None;
        }
        let marker = std::fs::read_to_string(self.runtime_marker()).ok()?;
        let (version, tag) = marker.trim().split_once(' ')?;
        (version.parse::<u32>().ok()? == RUNTIME_VERSION)
            .then(|| TorchVariant::from_tag(tag))
            .flatten()
    }

    pub(crate) fn write_runtime_marker(&self, variant: TorchVariant) -> std::io::Result<()> {
        std::fs::write(
            self.runtime_marker(),
            format!("{RUNTIME_VERSION} {}\n", variant.tag()),
        )
    }

    /// Ce qu'il faut effacer pour désinstaller le moteur en gardant les
    /// modèles.
    pub fn runtime_entries(&self) -> Vec<PathBuf> {
        vec![
            self.python_dir(),
            self.engine_dir(),
            self.runtime_marker(),
            self.downloads_dir(),
        ]
    }

    /// Tout ce que l'onglet a pu écrire ici, et rien d'autre : « Tout
    /// désinstaller » n'efface jamais un fichier qu'il n'a pas créé.
    pub fn owned_entries(&self) -> Vec<PathBuf> {
        let mut entries = self.runtime_entries();
        entries.push(self.models_dir());
        entries
    }

    /// Quelque chose est-il installé ici ? Tant que oui, on ne change pas de
    /// dossier.
    pub fn holds_anything(&self) -> bool {
        self.python_dir().exists() || self.models_dir().exists()
    }
}

/// Dossier des sons de travail de la session (source convertie, pistes).
pub fn work_dir() -> PathBuf {
    crate::media_binary::installation_temp_dir().join("processing")
}

/// Le moteur n'existe que pour Windows 64 bits (Python portable officiel).
pub fn platform_supported() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

/// Place occupée sur le disque par un dossier, en octets.
pub fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => directory_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// « 913 Mo », « 3,5 Go » : tailles lisibles dans la langue de l'interface.
pub fn format_bytes(bytes: u64) -> String {
    let megabytes = bytes as f64 / 1_000_000.0;
    let (value, unit) = if megabytes >= 1_000.0 {
        (megabytes / 1_000.0, crate::i18n::t("processing.unit.gb"))
    } else {
        (megabytes, crate::i18n::t("processing.unit.mb"))
    };
    let text = if value >= 100.0 || unit == crate::i18n::t("processing.unit.mb") {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    let text = if crate::config::language_or_default().starts_with("en") {
        text
    } else {
        text.replace('.', ",")
    };
    format!("{text} {unit}")
}

/// Ce que l'onglet demande à l'application. Les index de modèle suivent
/// [`CATALOG`], les index de piste l'ordre d'affichage.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessingAction {
    ImportAudio,
    UseProjectVideo,
    SelectItem(u64),
    RemoveItem(u64),
    InstallRuntime,
    UninstallRuntime,
    DownloadModel(usize),
    RemoveModel(usize),
    UninstallAll,
    OpenFolder,
    /// Choisir le dossier où installer le moteur et les modèles.
    ChooseDirectory,
    /// Arrêter l'installation du moteur.
    CancelEngine,
    /// Arrêter le téléchargement d'un modèle.
    CancelDownload(usize),
    SelectModel(usize),
    ToggleQuality,
    Run,
    Cancel,
    /// Écouter une piste, éventuellement à partir d'un instant précis.
    Listen {
        track: usize,
        at_ms: Option<u64>,
    },
    ExportTrack(usize),
    ExportAll,
    /// Enregistrer une piste dans un fichier et l'ajouter aux audios du
    /// projet.
    AddTrackToProject(usize),
    /// Traiter le son d'une vidéo du projet.
    SendProjectVideo(crate::project::MediaId),
    /// Traiter un audio du projet.
    SendProjectAudio(crate::project::MediaId),
    /// Retirer une piste produite, avec celles qui en descendent.
    RemoveTrack(usize),
    /// Écouter la piste précédente (-1) ou suivante (+1).
    ListenRelative(i32),
    /// Choisir le modèle installé précédent (-1) ou suivant (+1).
    SelectModelRelative(i32),
    /// Exporter la piste en écoute.
    ExportListened,
    /// Ajouter la piste en écoute aux audios du projet.
    AddListenedToProject,
    OpenCatalog,
    OpenEngine,
}

#[derive(Debug, Clone)]
pub struct Track {
    /// Fichier lu et exporté (stéréo, qualité d'origine).
    pub path: PathBuf,
    pub waveform: Arc<WaveformData>,
    /// Fréquence de l'analyse qui a produit la forme d'onde.
    pub waveform_sample_rate: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ResultStem {
    pub name: String,
    pub label_key: Option<&'static str>,
    pub track: Track,
}

/// Les pistes sorties d'un passage d'un modèle sur l'audio.
#[derive(Debug, Clone)]
pub struct ResultGroup {
    pub model: &'static ModelSpec,
    pub directory: PathBuf,
    pub stems: Vec<ResultStem>,
}

/// Une piste d'un audio : l'audio d'origine, ou une piste qu'un modèle a
/// tirée d'une autre. `parent` désigne celle dont elle descend.
#[derive(Debug, Clone)]
pub struct TrackNode {
    pub id: u64,
    pub parent: Option<u64>,
    /// Nom donné par le modèle ; vide pour l'audio d'origine.
    pub name: String,
    pub label_key: Option<&'static str>,
    /// Modèle qui l'a produite ; `None` pour l'audio d'origine.
    pub model: Option<&'static ModelSpec>,
    pub track: Track,
}

/// Un audio importé et tout ce qu'on en a tiré, en arbre : chaque piste
/// produite descend de celle qu'on a traitée, et peut être traitée à son
/// tour.
#[derive(Debug, Clone)]
pub struct AudioItem {
    pub id: u64,
    pub name: String,
    pub directory: PathBuf,
    nodes: Vec<TrackNode>,
    next_node: u64,
}

impl AudioItem {
    /// Identifiant de l'audio d'origine dans l'arbre.
    pub const ROOT: u64 = 0;

    pub fn new(id: u64, name: String, directory: PathBuf, source: Track) -> Self {
        Self {
            id,
            name,
            directory,
            nodes: vec![TrackNode {
                id: Self::ROOT,
                parent: None,
                name: String::new(),
                label_key: None,
                model: None,
                track: source,
            }],
            next_node: Self::ROOT + 1,
        }
    }

    pub fn root(&self) -> &TrackNode {
        &self.nodes[0]
    }

    pub fn node(&self, id: u64) -> Option<&TrackNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Nombre d'ancêtres : 0 pour l'audio d'origine.
    pub fn depth(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut current = self.node(id).and_then(|node| node.parent);
        while let Some(parent) = current {
            depth += 1;
            current = self.node(parent).and_then(|node| node.parent);
        }
        depth
    }

    fn children(&self, id: u64) -> impl Iterator<Item = &TrackNode> {
        self.nodes
            .iter()
            .filter(move |node| node.parent == Some(id))
    }

    /// Ordre d'affichage : chaque piste, suivie de celles qui en descendent.
    pub fn ordered(&self) -> Vec<u64> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![Self::ROOT];
        while let Some(id) = stack.pop() {
            order.push(id);
            let mut children: Vec<u64> = self.children(id).map(|node| node.id).collect();
            children.reverse();
            stack.extend(children);
        }
        order
    }

    /// La piste et toutes celles qui en descendent.
    pub fn subtree(&self, id: u64) -> Vec<u64> {
        let mut found = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if self.node(current).is_none() {
                continue;
            }
            found.push(current);
            stack.extend(self.children(current).map(|node| node.id));
        }
        found
    }

    pub fn has_results(&self) -> bool {
        self.nodes.len() > 1
    }

    /// Range sous `parent` les pistes qu'un modèle vient d'en tirer. Un
    /// nouveau passage du même modèle remplace ses anciennes pistes, sauf
    /// celles qui ont déjà été traitées à leur tour. Rend les nouvelles
    /// pistes et les fichiers à effacer.
    pub fn add_results(&mut self, parent: u64, group: ResultGroup) -> (Vec<u64>, Vec<PathBuf>) {
        if self.node(parent).is_none() {
            return (
                Vec::new(),
                group
                    .stems
                    .into_iter()
                    .map(|stem| stem.track.path)
                    .collect(),
            );
        }
        let replaced: Vec<u64> = self
            .children(parent)
            .filter(|node| node.model.is_some_and(|model| model.id == group.model.id))
            .filter(|node| self.children(node.id).next().is_none())
            .map(|node| node.id)
            .collect();
        let mut files = Vec::new();
        for id in replaced {
            files.extend(self.remove(id));
        }
        let mut added = Vec::with_capacity(group.stems.len());
        for stem in group.stems {
            let id = self.next_node;
            self.next_node += 1;
            self.nodes.push(TrackNode {
                id,
                parent: Some(parent),
                name: stem.name,
                label_key: stem.label_key,
                model: Some(group.model),
                track: stem.track,
            });
            added.push(id);
        }
        (added, files)
    }

    /// Retire une piste produite et celles qui en descendent. Rend leurs
    /// fichiers. L'audio d'origine ne se retire pas ainsi.
    pub fn remove(&mut self, id: u64) -> Vec<PathBuf> {
        if id == Self::ROOT {
            return Vec::new();
        }
        let doomed = self.subtree(id);
        let mut files = Vec::new();
        self.nodes.retain(|node| {
            if doomed.contains(&node.id) {
                files.push(node.track.path.clone());
                false
            } else {
                true
            }
        });
        files
    }
}

/// Les audios de la session. Rien n'est enregistré dans le projet : les
/// pistes vivent dans le dossier temporaire jusqu'à leur export.
#[derive(Debug, Default)]
pub struct ProcessingSession {
    items: Vec<AudioItem>,
    active: Option<u64>,
    next_id: u64,
}

impl ProcessingSession {
    pub fn items(&self) -> &[AudioItem] {
        &self.items
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active
    }

    pub fn active(&self) -> Option<&AudioItem> {
        let id = self.active?;
        self.items.iter().find(|item| item.id == id)
    }

    pub fn item(&self, id: u64) -> Option<&AudioItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn item_mut(&mut self, id: u64) -> Option<&mut AudioItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub fn allocate_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn add(&mut self, item: AudioItem) {
        self.active = Some(item.id);
        self.items.push(item);
    }

    pub fn select(&mut self, id: u64) -> bool {
        if self.active == Some(id) || self.item(id).is_none() {
            return false;
        }
        self.active = Some(id);
        true
    }

    /// Retire un audio et rend son dossier de travail à supprimer.
    pub fn remove(&mut self, id: u64) -> Option<AudioItem> {
        let index = self.items.iter().position(|item| item.id == id)?;
        let item = self.items.remove(index);
        if self.active == Some(id) {
            self.active = self
                .items
                .get(index.min(self.items.len().saturating_sub(1)))
                .map(|item| item.id);
        }
        Some(item)
    }
}

/// Nom de fichier sûr pour l'export d'une piste.
pub fn export_file_stem(audio_name: &str, stem_label: &str) -> String {
    let base = Path::new(audio_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| audio_name.to_owned());
    crate::voicelines::export_stem(&format!("{base} - {stem_label}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str) -> Track {
        Track {
            path: PathBuf::from(path),
            waveform: Arc::new(WaveformData::default()),
            waveform_sample_rate: 48_000,
            duration_ms: 1_000,
        }
    }

    fn item(id: u64) -> AudioItem {
        AudioItem::new(
            id,
            format!("audio{id}.wav"),
            PathBuf::from(format!("work/{id}")),
            track("source.wav"),
        )
    }

    fn group(model: usize, directory: &str, stems: usize) -> ResultGroup {
        ResultGroup {
            model: &CATALOG[model],
            directory: PathBuf::from(directory),
            stems: (0..stems)
                .map(|index| ResultStem {
                    name: format!("stem{index}"),
                    label_key: None,
                    track: track(&format!("{directory}/stem{index}.flac")),
                })
                .collect(),
        }
    }

    #[test]
    fn the_vocal_remover_is_the_verified_gabox_checkpoint() {
        let model = &CATALOG[0];
        assert!(model
            .checkpoint_url
            .ends_with("/melbandroformers/instrumental/Inst_GaboxFv7z.ckpt"));
        assert!(model
            .config_url
            .ends_with("/melbandroformers/instrumental/inst_gabox.yaml"));
        assert_eq!(model.checkpoint_bytes, 913_031_195);
        assert_eq!(
            model.stem_label_key("vocals"),
            Some("processing.stem.vocals")
        );
    }

    #[test]
    fn everything_installs_in_a_subfolder_of_the_chosen_folder() {
        let location = InstallLocation::from_chosen(Path::new(r"D:\IA"));
        assert_eq!(location.root(), Path::new(r"D:\IA\coquerythmo-processing"));
        for path in location.owned_entries() {
            assert!(path.starts_with(location.root()), "{}", path.display());
            assert_ne!(path, location.root());
        }
        assert!(location
            .checkpoint_path(&CATALOG[0])
            .starts_with(location.models_dir()));
        // Redésigner le sous-dossier lui-même ne l'imbrique pas une 2e fois.
        assert_eq!(InstallLocation::from_chosen(location.root()), location);
    }

    #[test]
    fn a_folder_is_checked_then_tells_what_it_holds() {
        let folder =
            std::env::temp_dir().join(format!("coquerythmo-location-test-{}", std::process::id()));
        let location = InstallLocation::from_chosen(&folder);
        assert!(location.is_writable());
        assert!(!location.holds_anything());
        assert_eq!(location.installed_runtime(), None);
        assert!(!location.has_model(&CATALOG[0]));

        std::fs::create_dir_all(location.python_dir()).unwrap();
        std::fs::write(location.python_exe(), b"").unwrap();
        location
            .write_runtime_marker(TorchVariant::Cuda128)
            .unwrap();
        assert!(location.holds_anything());
        assert_eq!(location.installed_runtime(), Some(TorchVariant::Cuda128));
        std::fs::write(location.runtime_marker(), "0 cu128\n").unwrap();
        assert_eq!(location.installed_runtime(), None);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn catalog_ids_and_files_are_unique() {
        for (index, model) in CATALOG.iter().enumerate() {
            for other in &CATALOG[index + 1..] {
                assert_ne!(model.id, other.id);
                assert_ne!(model.checkpoint_file, other.checkpoint_file);
            }
            assert!(model.checkpoint_url.starts_with("https://"));
            assert!(model.checkpoint_url.ends_with(model.checkpoint_file));
            assert!(model.config_url.ends_with(model.config_file));
        }
    }

    #[test]
    fn gpu_line_selects_the_matching_torch_build() {
        let recent = parse_gpu_line("NVIDIA GeForce RTX 4060, 8188").unwrap();
        assert_eq!(recent.vram_mb, 8_188);
        assert_eq!(recent.torch_variant(), TorchVariant::Cuda128);
        assert!(recent.is_recommended());

        let old = parse_gpu_line("NVIDIA GeForce GTX 1060 6GB, 6144").unwrap();
        assert_eq!(old.torch_variant(), TorchVariant::Cuda126);
        assert!(!old.is_recommended());
        assert!(parse_gpu_line("").is_none());
    }

    #[test]
    fn torch_wheel_keeps_the_exact_file_name_pip_expects() {
        assert_eq!(
            TorchVariant::Cuda128.wheel_file(),
            "torch-2.8.0+cu128-cp311-cp311-win_amd64.whl"
        );
        assert!(TorchVariant::Cuda128
            .wheel_url()
            .ends_with("/cu128/torch-2.8.0%2Bcu128-cp311-cp311-win_amd64.whl"));
        assert_eq!(TorchVariant::from_tag("cpu"), Some(TorchVariant::Cpu));
    }

    #[test]
    fn a_processed_track_can_be_processed_again_and_keeps_its_lineage() {
        let mut audio = item(1);
        let (first, _) = audio.add_results(AudioItem::ROOT, group(0, "a", 2));
        // The instrumental is split again: its tracks descend from it.
        let (second, _) = audio.add_results(first[0], group(2, "b", 3));
        assert_eq!(audio.node(second[0]).unwrap().parent, Some(first[0]));
        assert_eq!(audio.depth(AudioItem::ROOT), 0);
        assert_eq!(audio.depth(first[1]), 1);
        assert_eq!(audio.depth(second[2]), 2);
        // Display order: a track, then its descendants, then its siblings.
        assert_eq!(
            audio.ordered(),
            vec![
                AudioItem::ROOT,
                first[0],
                second[0],
                second[1],
                second[2],
                first[1]
            ]
        );
    }

    #[test]
    fn each_produced_track_is_removed_on_its_own_with_its_descendants() {
        let mut audio = item(1);
        let (first, _) = audio.add_results(AudioItem::ROOT, group(0, "a", 2));
        let (second, _) = audio.add_results(first[0], group(2, "b", 2));

        // The voice goes, the instrumental and what came from it stay.
        assert_eq!(audio.remove(first[1]), vec![PathBuf::from("a/stem1.flac")]);
        assert_eq!(
            audio.ordered(),
            vec![AudioItem::ROOT, first[0], second[0], second[1]]
        );

        // Removing the instrumental takes its descendants with it.
        let mut files = audio.remove(first[0]);
        files.sort();
        assert_eq!(
            files,
            vec![
                PathBuf::from("a/stem0.flac"),
                PathBuf::from("b/stem0.flac"),
                PathBuf::from("b/stem1.flac"),
            ]
        );
        assert!(!audio.has_results());
        // The original audio is not removed this way.
        assert!(audio.remove(AudioItem::ROOT).is_empty());
        assert_eq!(audio.ordered(), vec![AudioItem::ROOT]);
    }

    #[test]
    fn a_second_pass_replaces_the_tracks_that_were_not_processed_further() {
        let mut audio = item(1);
        let (first, _) = audio.add_results(AudioItem::ROOT, group(0, "a", 2));
        audio.add_results(first[0], group(2, "b", 1));
        let (again, files) = audio.add_results(AudioItem::ROOT, group(0, "c", 2));
        // The untouched voice is replaced; the instrumental that has
        // descendants is kept.
        assert_eq!(files, vec![PathBuf::from("a/stem1.flac")]);
        assert!(audio.node(first[0]).is_some());
        assert!(audio.node(first[1]).is_none());
        assert_eq!(again.len(), 2);
        // Tracks of a parent that is gone are handed back for deletion.
        let (added, files) = audio.add_results(999, group(0, "d", 2));
        assert!(added.is_empty());
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn the_four_utility_models_carry_the_stems_of_their_configuration() {
        let stems = |id: &str| -> Vec<&str> {
            CATALOG
                .iter()
                .find(|model| model.id == id)
                .unwrap_or_else(|| panic!("model {id} missing"))
                .stems
                .iter()
                .map(|stem| stem.name)
                .collect()
        };
        assert_eq!(stems("aspiration"), ["aspiration", "other"]);
        assert_eq!(stems("crowd"), ["crowd", "other"]);
        assert_eq!(stems("male_female"), ["male", "female"]);
        assert_eq!(stems("karaoke_small"), ["Vocals", "Instrumental"]);
    }

    #[test]
    fn removing_the_active_audio_selects_its_neighbour() {
        let mut session = ProcessingSession::default();
        session.add(item(1));
        session.add(item(2));
        assert_eq!(session.active_id(), Some(2));
        assert!(session.remove(2).is_some());
        assert_eq!(session.active_id(), Some(1));
        assert!(session.remove(1).is_some());
        assert_eq!(session.active_id(), None);
    }
}
