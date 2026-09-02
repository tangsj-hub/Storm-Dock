#!/usr/bin/env python3
"""Generate platform asset manifests from the checked-in Unsloth source."""
import ast, hashlib, json, re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UNSLOTH = ROOT / "unsloth/studio/backend/core/inference"
OUT = ROOT / "model-platforms/unsloth.json"
SOURCES = [UNSLOTH / "video_families.py", UNSLOTH / "diffusion_families.py", UNSLOTH / "video_minimax_h3.py"]
REPO_RE = re.compile(r"\b(?:[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)\b")

def strings(node):
    return [n.value for n in ast.walk(node) if isinstance(n, ast.Constant) and isinstance(n.value, str)]

def main():
    text = "\n".join(p.read_text(encoding="utf-8") for p in SOURCES)
    repos = set()
    assets = {}
    for match in REPO_RE.finditer(text):
        value = match.group(0)
        if "/" in value and not value.startswith(("http", "test/")):
            repos.add(value)
    for path in SOURCES:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign):
                value = node.value
                values = strings(value)
                for i in range(0, len(values) - 1):
                    candidate = values[i + 1]
                    is_file = candidate.startswith(("vae/", "split_files/")) or candidate.lower().endswith((".gguf", ".safetensors", ".bin", ".pt", ".pth", ".json", ".model"))
                    if "/" in values[i] and values[i] in repos and candidate not in repos and is_file:
                        assets.setdefault(values[i], set()).add(candidate)
    h3_repo = re.search(r"H3_GGUF_REPO\s*=\s*[\"']([^\"']+)[\"']", text)
    if h3_repo:
        for name in ("H3_VIDEO_VAE", "H3_AUDIO_VAE", "H3_QWEN_Q2", "H3_QWEN_Q4"):
            match = re.search(rf"{name}\s*=\s*[\"']([^\"']+)[\"']", text)
            if match:
                assets.setdefault(h3_repo.group(1), set()).add(match.group(1))
    repositories = {}
    for repo, paths in sorted(assets.items()):
        quant_assets = {}
        common = []
        for path in sorted(paths):
            match = re.search(r"(q[234568]_[a-z0-9_]+)", path.lower())
            if match:
                quant_assets.setdefault(match.group(1), []).append(path)
            else:
                common.append(path)
        repositories[repo] = {"assets": common, "quantAssets": quant_assets}
    payload = {
        "schemaVersion": 1,
        "platform": "unsloth",
        "generatedFrom": [str(p.relative_to(ROOT)) for p in SOURCES],
        "sourceSha256": hashlib.sha256(text.encode()).hexdigest(),
        "generic": {
            "auxiliary": ["config.json", "configuration.json", "generation_config.json", "tokenizer*", "chat_template*", "special_tokens_map.json", "added_tokens.json", "vocab*", "merges.txt", "spiece.model", "spm.model", "normalizer.json", "preprocessor_config.json", "processor_config.json", "video_preprocessor_config.json", "*.tiktoken", "*.py"],
            "weights": ["*.safetensors", "*.safetensors.index.json", "pytorch_model*.bin", "pytorch_model*.bin.index.json"]
        },
        "repositories": repositories,
    }
    OUT.write_text(json.dumps(payload, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")

if __name__ == "__main__":
    main()
