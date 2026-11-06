"""Fine-tune Squad's model with LoRA on chat examples, then merge it.

    python train_lora.py --data synth.jsonl mine.jsonl --out peek-lora

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


def old_gpu() -> None:
    """GPUs older than Ampere (a T4 is 7.5) can't multiply half-precision
    numbers into a float32 result in one step, which TRL's chunked loss does
    by default. Tell it to widen the numbers first instead."""
    if not torch.cuda.is_available() or torch.cuda.get_device_capability()[0] >= 8:
        return
    try:
        import trl.kernels.chunked_logprob as chunked

        chunked._MM_OUT_DTYPE = False
        print("Older GPU: TRL's loss widens to float32 before multiplying.")
    except ImportError:
        pass


def lora_targets(model) -> list[str]:
    """The text model's linear layers, by name. "all-linear" also reaches a
    model's vision tower and odd layers some PEFT versions can't wrap."""
    names = set()
    for full, mod in model.named_modules():
        if not isinstance(mod, torch.nn.Linear):
            continue
        if any(part in full for part in ("visual", "vision", "lm_head", "mtp")):
            continue
        names.add(full.rsplit(".", 1)[-1])
    print("LoRA on:", ", ".join(sorted(names)))
    return sorted(names)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="Qwen/Qwen3.5-0.8B")
    ap.add_argument("--data", nargs="+", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=float, default=2)
    ap.add_argument("--rank", type=int, default=32)
    args = ap.parse_args()

    # is_bf16_supported() says yes on a T4 too (it counts emulation), so ask
    # for Ampere or newer, where bfloat16 is native.
    bf16 = torch.cuda.is_available() and torch.cuda.get_device_capability()[0] >= 8
    fp16 = torch.cuda.is_available() and not bf16
    old_gpu()

    tok = AutoTokenizer.from_pretrained(args.base)
    # Newer transformers call it `dtype` and default to the checkpoint's
    # bfloat16, which a T4 can't train in; older ones call it `torch_dtype`.
    dtype = torch.bfloat16 if bf16 else torch.float32
    try:
        model = AutoModelForCausalLM.from_pretrained(args.base, dtype=dtype)
    except TypeError:
        model = AutoModelForCausalLM.from_pretrained(args.base, torch_dtype=dtype)
    model.to(dtype)
    data = load_dataset("json", data_files=args.data, split="train").shuffle(seed=7)
    # Prompt (instructions + the person's words) and completion (Peek's answer),
    # so only the answer is learned.
    data = data.map(
        lambda r: {"prompt": r["messages"][:-1], "completion": r["messages"][-1:]},
        remove_columns=["messages"],
    )
    trainer = SFTTrainer(
        model=model,
        processing_class=tok,
        train_dataset=data,
        peft_config=LoraConfig(
            r=args.rank,
            lora_alpha=args.rank * 2,
            lora_dropout=0.05,
            target_modules=lora_targets(model),
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
            # Saved often, so a run that stops carries on from where it was.
            save_strategy="steps",
            save_steps=200,
            save_total_limit=2,
            completion_only_loss=True,
            max_length=1024,
            bf16=bf16,
            fp16=fp16,
            report_to="none",
        ),
    )
    import glob

    resume = bool(glob.glob(f"{args.out}/checkpoint-*"))
    if resume:
        print("Carrying on from the last checkpoint in", args.out)
    trainer.train(resume_from_checkpoint=resume or None)
    merged = trainer.model.merge_and_unload()
    merged.save_pretrained(f"{args.out}/merged")
    tok.save_pretrained(f"{args.out}/merged")
    print(f"Merged model in {args.out}/merged. Next: convert to GGUF (see README).")


if __name__ == "__main__":
    main()
