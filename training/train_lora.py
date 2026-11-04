"""Fine-tune Squad's model with LoRA on chat examples, then merge it.

    python train_lora.py --data synth.jsonl mine.jsonl --out fin-lora

A 0.8B model with LoRA fits on one 16 GB GPU (a free Colab or Kaggle T4
works, slowly). On a Mac, MLX-LM's `mlx_lm.lora` does the same job with the
same JSONL files.
"""

import argparse

import torch
from datasets import load_dataset
from peft import LoraConfig
from transformers import AutoModelForCausalLM, AutoTokenizer
from trl import SFTConfig, SFTTrainer


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
        torch_dtype=torch.bfloat16 if torch.cuda.is_available() else torch.float32,
    )
    data = load_dataset("json", data_files=args.data, split="train").shuffle(seed=7)

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
        args=SFTConfig(
            output_dir=args.out,
            num_train_epochs=args.epochs,
            per_device_train_batch_size=16,
            gradient_accumulation_steps=2,
            learning_rate=2e-4,
            lr_scheduler_type="cosine",
            warmup_ratio=0.05,
            logging_steps=20,
            save_strategy="epoch",
            # Learn the answers, not the instructions.
            assistant_only_loss=True,
            max_length=1024,
            bf16=torch.cuda.is_available(),
        ),
    )
    trainer.train()
    merged = trainer.model.merge_and_unload()
    merged.save_pretrained(f"{args.out}/merged")
    tok.save_pretrained(f"{args.out}/merged")
    print(f"Merged model in {args.out}/merged. Next: convert to GGUF (see README).")


if __name__ == "__main__":
    main()
