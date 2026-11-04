"""Fine-tune Squad's model with LoRA on chat examples, then merge it.

    python train_lora.py --data synth.jsonl mine.jsonl --out fin-lora

A 0.8B model with LoRA fits on one 16 GB GPU; a free Colab or Kaggle T4
works (see colab.ipynb). On a Mac, MLX-LM's `mlx_lm.lora` does the same job with the
same JSONL files.
"""

import argparse
import inspect

import torch
from datasets import load_dataset
from peft import LoraConfig
from transformers import AutoModelForCausalLM, AutoTokenizer
from trl import SFTConfig, SFTTrainer


def config(**kw) -> SFTConfig:
    """An SFTConfig from the settings this version of TRL knows; the names
    move between releases, and a missing one shouldn't stop the run."""
    known = set(inspect.signature(SFTConfig.__init__).parameters)
    dropped = sorted(k for k in kw if k not in known)
    if dropped:
        print("This TRL version has no", ", ".join(dropped), "- leaving it out.")
    return SFTConfig(**{k: v for k, v in kw.items() if k in known})


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="Qwen/Qwen3.5-0.8B")
    ap.add_argument("--data", nargs="+", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=float, default=2)
    ap.add_argument("--rank", type=int, default=16)
    args = ap.parse_args()

    tok = AutoTokenizer.from_pretrained(args.base)
    model = AutoModelForCausalLM.from_pretrained(
        args.base,
        torch_dtype=torch.bfloat16
        if torch.cuda.is_available() and torch.cuda.is_bf16_supported()
        else torch.float32,
    )
    data = load_dataset("json", data_files=args.data, split="train").shuffle(seed=7)
    # Prompt (instructions + the person's words) and completion (Fin's answer),
    # so only the answer is learned.
    data = data.map(
        lambda r: {"prompt": r["messages"][:-1], "completion": r["messages"][-1:]},
        remove_columns=["messages"],
    )
    bf16 = torch.cuda.is_available() and torch.cuda.is_bf16_supported()
    fp16 = torch.cuda.is_available() and not bf16

    trainer = SFTTrainer(
        model=model,
        processing_class=tok,
        train_dataset=data,
        peft_config=LoraConfig(
            r=args.rank,
            lora_alpha=args.rank * 2,
            lora_dropout=0.05,
            target_modules="all-linear",
            task_type="CAUSAL_LM",
        ),
        args=config(
            output_dir=args.out,
            num_train_epochs=args.epochs,
            per_device_train_batch_size=16,
            gradient_accumulation_steps=2,
            learning_rate=2e-4,
            lr_scheduler_type="cosine",
            warmup_steps=10,
            logging_steps=20,
            save_strategy="epoch",
            completion_only_loss=True,
            max_length=1024,
            bf16=bf16,
            fp16=fp16,
            report_to="none",
        ),
    )
    trainer.train()
    merged = trainer.model.merge_and_unload()
    merged.save_pretrained(f"{args.out}/merged")
    tok.save_pretrained(f"{args.out}/merged")
    print(f"Merged model in {args.out}/merged. Next: convert to GGUF (see README).")


if __name__ == "__main__":
    main()
