# Coquerythmo - moteur de l'onglet Traitement.
#
# Lance par l'application avec le Python portable installe depuis l'onglet.
# Il parle a l'application en JSON, une ligne par evenement, sur la sortie
# standard. Le son est lu et ecrit par blocs : un film entier ne tient jamais
# en memoire.
import argparse
import importlib.util
import json
import os
import sys
import time
import types


def emit(event, **data):
    data["event"] = event
    sys.stdout.write(json.dumps(data, ensure_ascii=True) + "\n")
    sys.stdout.flush()


def fail(message):
    emit("error", message=str(message))
    sys.exit(1)


def device_report(torch):
    report = {"torch": torch.__version__, "cuda": bool(torch.cuda.is_available())}
    if report["cuda"]:
        _, total = torch.cuda.mem_get_info()
        report["gpu"] = torch.cuda.get_device_name(0)
        report["vram_mb"] = int(total // (1024 * 1024))
    return report


def import_loader():
    # Le paquet audio-separator charge toutes ses architectures (ONNX, Demucs...)
    # des qu'on l'importe. On ne veut que ses classes Roformer : un faux paquet
    # parent evite d'executer son __init__ et ses dependances inutiles.
    spec = importlib.util.find_spec("audio_separator")
    if spec is None or not spec.origin:
        raise RuntimeError("audio-separator is not installed")
    root = os.path.join(os.path.dirname(spec.origin), "separator")
    stub = types.ModuleType("audio_separator.separator")
    stub.__path__ = [root]
    sys.modules["audio_separator.separator"] = stub
    from audio_separator.separator.roformer.roformer_loader import RoformerLoader

    return RoformerLoader


def honor_mask_estimator_width(config):
    # audio-separator 0.48.0 accepte `mlp_expansion_factor` pour un Mel-Band
    # Roformer mais ne le transmet pas a ses estimateurs de masque : un modele
    # allege (facteur 1 au lieu de 4) ne correspond alors plus a ses poids.
    # On le transmet a sa place.
    model_cfg = config.get("model") or {}
    factor = int(model_cfg.get("mlp_expansion_factor") or 4)
    if "num_bands" not in model_cfg or factor == 4:
        return
    module = importlib.import_module(
        "audio_separator.separator.uvr_lib_v5.roformer.mel_band_roformer"
    )
    original = module.MaskEstimator
    if getattr(original, "_coquerythmo_width", None) == factor:
        return

    class MaskEstimator(original):
        _coquerythmo_width = factor

        def __init__(self, *args, **kwargs):
            kwargs.setdefault("mlp_expansion_factor", factor)
            super().__init__(*args, **kwargs)

    module.MaskEstimator = MaskEstimator


def chunk_starts(total, chunk, step):
    starts = []
    for offset in range(0, total, step):
        if offset + chunk >= total:
            tail = max(total - chunk, 0)
            if not starts or starts[-1] != tail:
                starts.append(tail)
            break
        starts.append(offset)
    return starts or [0]


def stem_names(config):
    training = config.get("training") or {}
    instruments = [str(name) for name in (training.get("instruments") or [])]
    target = training.get("target_instrument")
    if target:
        target = str(target)
        others = [name for name in instruments if name != target]
        residual = others[0] if len(others) == 1 else "other"
        return [target], residual
    if not instruments:
        raise RuntimeError("the model configuration lists no instrument")
    return instruments, None


def pick_batch(torch, device, requested):
    if requested > 0:
        return requested
    if device.type != "cuda":
        return 1
    _, total = torch.cuda.mem_get_info()
    gigabytes = total / (1024 ** 3)
    if gigabytes >= 22:
        return 4
    if gigabytes >= 11:
        return 2
    return 1


def is_out_of_memory(torch, error):
    if isinstance(error, getattr(torch.cuda, "OutOfMemoryError", ())):
        return True
    return "out of memory" in str(error).lower()


def separate(args):
    started = time.time()
    emit("stage", name="loading")
    import numpy as np
    import soundfile as sf
    import torch
    import yaml

    with open(args.config, "r", encoding="utf-8") as handle:
        config = yaml.load(handle, Loader=yaml.FullLoader)

    use_cuda = torch.cuda.is_available() and args.device != "cpu"
    device = torch.device("cuda" if use_cuda else "cpu")
    if use_cuda:
        torch.backends.cuda.matmul.allow_tf32 = True
        torch.backends.cudnn.allow_tf32 = True
        torch.backends.cudnn.benchmark = True
    torch.set_float32_matmul_precision("high")
    emit("device", **device_report(torch))

    loader = import_loader()()
    honor_mask_estimator_width(config)
    loaded = loader.load_model(model_path=args.model, config=config, device=str(device))
    model = getattr(loaded, "model", None)
    if not getattr(loaded, "success", False) or model is None:
        raise RuntimeError(getattr(loaded, "error_message", None) or "model loading failed")
    model.eval()

    audio_cfg = config.get("audio") or {}
    model_cfg = config.get("model") or {}
    inference_cfg = config.get("inference") or {}
    sample_rate = int(audio_cfg.get("sample_rate") or model_cfg.get("sample_rate") or 44100)
    chunk = int(audio_cfg.get("chunk_size") or 0)
    if chunk <= 0:
        hop = int(model_cfg.get("stft_hop_length") or audio_cfg.get("hop_length") or 441)
        chunk = hop * (int(inference_cfg.get("dim_t") or 801) - 1)
    overlap = max(1, int(args.overlap or inference_cfg.get("num_overlap") or 2))
    channels = 2 if model_cfg.get("stereo", True) else 1
    outputs, residual = stem_names(config)

    source = sf.SoundFile(args.input)
    if source.samplerate != sample_rate or source.channels != channels:
        raise RuntimeError(
            "expected %d Hz / %d channels, got %d Hz / %d channels"
            % (sample_rate, channels, source.samplerate, source.channels)
        )
    total = int(source.frames)
    if total <= 0:
        raise RuntimeError("the audio file is empty")

    os.makedirs(args.output_dir, exist_ok=True)
    names = outputs + ([residual] if residual else [])
    paths = [os.path.join(args.output_dir, "%02d_%s.flac" % (i, n)) for i, n in enumerate(names)]
    writers = [
        sf.SoundFile(path, "w", samplerate=sample_rate, channels=channels, format="FLAC", subtype="PCM_24")
        for path in paths
    ]

    batch = pick_batch(torch, device, args.batch)
    use_amp = device.type == "cuda" and not args.no_amp

    def read_block(start, length):
        source.seek(start)
        block = source.read(length, dtype="float32", always_2d=True)
        if block.shape[0] < length:
            block = np.concatenate(
                [block, np.zeros((length - block.shape[0], channels), dtype=np.float32)]
            )
        return np.ascontiguousarray(block.T)

    def run(parts):
        tensor = torch.from_numpy(np.stack(parts)).to(device)
        with torch.inference_mode():
            if use_amp:
                with torch.autocast(device_type="cuda", dtype=torch.float16):
                    out = model(tensor)
            else:
                out = model(tensor)
        out = out.float()
        if out.dim() == 3:
            out = out.unsqueeze(1)
        return out.cpu().numpy()

    emit("stage", name="separating", batch=batch, chunk=chunk, overlap=overlap, amp=use_amp)

    # `written` compte les echantillons definitifs, deja sur le disque. Chaque
    # passe de la boucle externe traite ce qui reste avec les reglages
    # courants : on n'y revient que si la carte graphique manque de memoire.
    written = 0
    while written < total:
        step = max(chunk // overlap, 1)
        starts = [written + s for s in chunk_starts(total - written, chunk, step)]
        window = np.hamming(chunk).astype(np.float32)
        capacity = chunk + step * batch
        acc = np.zeros((len(outputs), channels, capacity), dtype=np.float32)
        weight = np.zeros(capacity, dtype=np.float32)
        base = written
        index = 0
        while index < len(starts):
            group = starts[index:index + batch]
            parts = [read_block(start, chunk) for start in group]
            try:
                result = run(parts)
            except RuntimeError as error:
                if not is_out_of_memory(torch, error):
                    raise
                del parts
                torch.cuda.empty_cache()
                if batch > 1:
                    batch = max(1, batch // 2)
                    emit("notice", code="batch_reduced", batch=batch)
                elif chunk > sample_rate * 2:
                    chunk = max(sample_rate * 2, chunk // 2)
                    emit("notice", code="chunk_reduced", chunk=chunk)
                else:
                    raise RuntimeError("not enough video memory for this model")
                break
            for row, start in enumerate(group):
                offset = start - base
                length = min(chunk, result.shape[-1], total - start)
                if length <= 0:
                    continue
                acc[:, :, offset:offset + length] += result[row][:, :, :length] * window[:length]
                weight[offset:offset + length] += window[:length]
            index += len(group)
            # Les morceaux suivants commencent a `limit` : tout ce qui precede
            # ne changera plus et peut partir sur le disque.
            limit = starts[index] if index < len(starts) else total
            count = limit - base
            if count > 0:
                block = acc[:, :, :count] / np.maximum(weight[:count], 1e-8)
                np.clip(block, -1.0, 1.0, out=block)
                for stem, writer in enumerate(writers[:len(outputs)]):
                    writer.write(np.ascontiguousarray(block[stem].T))
                if residual:
                    rest = read_block(base, count) - block[0]
                    np.clip(rest, -1.0, 1.0, out=rest)
                    writers[-1].write(np.ascontiguousarray(rest.T))
                acc[:, :, :capacity - count] = acc[:, :, count:]
                acc[:, :, capacity - count:] = 0.0
                weight[:capacity - count] = weight[count:]
                weight[capacity - count:] = 0.0
                base = limit
                written = limit
            emit(
                "progress",
                done=written,
                total=total,
                elapsed=round(time.time() - started, 2),
            )

    for writer in writers:
        writer.close()
    source.close()
    for name, path in zip(names, paths):
        emit("stem", name=name, path=path)
    emit(
        "done",
        seconds=round(time.time() - started, 2),
        audio_seconds=round(total / float(sample_rate), 2),
    )


def probe(_args):
    import torch

    # Charger les deux familles de modeles prouve que toutes leurs
    # bibliotheques sont la : une installation incomplete echoue ici, pas au
    # premier traitement.
    import_loader()
    importlib.import_module("audio_separator.separator.uvr_lib_v5.roformer.mel_band_roformer")
    importlib.import_module("audio_separator.separator.uvr_lib_v5.roformer.bs_roformer")
    import numpy  # noqa: F401
    import soundfile  # noqa: F401
    import yaml  # noqa: F401

    emit("device", **device_report(torch))
    emit("done")


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("probe")
    run = commands.add_parser("separate")
    run.add_argument("--model", required=True)
    run.add_argument("--config", required=True)
    run.add_argument("--input", required=True)
    run.add_argument("--output-dir", required=True)
    run.add_argument("--batch", type=int, default=0)
    run.add_argument("--overlap", type=int, default=0)
    run.add_argument("--device", default="auto")
    run.add_argument("--no-amp", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "probe":
            probe(args)
        else:
            separate(args)
    except SystemExit:
        raise
    except BaseException as error:  # noqa: BLE001 - tout doit remonter a l'application
        fail("%s: %s" % (type(error).__name__, error))


if __name__ == "__main__":
    main()
