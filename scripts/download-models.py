#!/usr/bin/env python3
"""
Download and convert OPUS-MT models for CTranslate2.
Usage: python scripts/download-models.py [--model es-en] [--all]
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

MODELS = {
    "es-en": "Helsinki-NLP/opus-mt-es-en",
    "pt-en": "Helsinki-NLP/opus-mt-pt-en",
    "zh-en": "Helsinki-NLP/opus-mt-zh-en",
    "ru-en": "Helsinki-NLP/opus-mt-ru-en",
    "de-en": "Helsinki-NLP/opus-mt-de-en",
    "fr-en": "Helsinki-NLP/opus-mt-fr-en",
    "ko-en": "Helsinki-NLP/opus-mt-ko-en",
    "ja-en": "Helsinki-NLP/opus-mt-ja-en",
    "tr-en": "Helsinki-NLP/opus-mt-tr-en",
    "ar-en": "Helsinki-NLP/opus-mt-ar-en",
}

def check_dependencies():
    """Check if required Python packages are installed."""
    try:
        import ctranslate2
        import transformers
        import sentencepiece
    except ImportError as e:
        print(f"Missing dependency: {e}")
        print("Install with: pip install ctranslate2 transformers sentencepiece")
        sys.exit(1)

def download_model(model_name: str, output_dir: Path, quantization: str = "int8"):
    """Download and convert a single model."""
    hf_name = MODELS.get(model_name)
    if not hf_name:
        print(f"Unknown model: {model_name}")
        print(f"Available: {', '.join(MODELS.keys())}")
        return False
    
    output_path = output_dir / f"opus-mt-{model_name}-ct2"
    
    if output_path.exists() and (output_path / "model.bin").exists():
        print(f"Model {model_name} already exists at {output_path}")
        return True
    
    print(f"Downloading {hf_name}...")
    
    try:
        subprocess.run([
            "ct2-transformers-converter",
            "--model", hf_name,
            "--output_dir", str(output_path),
            "--quantization", quantization,
        ], check=True)
        print(f"Converted {model_name} -> {output_path}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"Failed to convert {model_name}: {e}")
        return False

def main():
    parser = argparse.ArgumentParser(description="Download OPUS-MT models for CTranslate2")
    parser.add_argument("--model", choices=MODELS.keys(), help="Specific model to download")
    parser.add_argument("--all", action="store_true", help="Download all models")
    parser.add_argument("--output", type=Path, default=Path("models"), help="Output directory")
    parser.add_argument("--quantization", default="int8", choices=["int8", "float16", "float32"])
    
    args = parser.parse_args()
    
    check_dependencies()
    
    args.output.mkdir(parents=True, exist_ok=True)
    
    if args.all:
        models_to_download = list(MODELS.keys())
    elif args.model:
        models_to_download = [args.model]
    else:
        print("Specify --model or --all")
        sys.exit(1)
    
    success = 0
    for model in models_to_download:
        if download_model(model, args.output, args.quantization):
            success += 1
    
    print(f"\nDownloaded {success}/{len(models_to_download)} models to {args.output.absolute()}")

if __name__ == "__main__":
    main()
