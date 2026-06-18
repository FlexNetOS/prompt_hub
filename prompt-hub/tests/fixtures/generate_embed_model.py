"""Generate a tiny deterministic ONNX embedding model for offline tests.

The model accepts two int64 inputs:
  - input_ids:      [batch, seq]
  - attention_mask: [batch, seq]
and produces a float32 output:
  - embeddings:     [batch, dim]

It embeds each input_id by looking it up in a small constant table (after
modulo vocab_size) and returns the mean of the masked token embeddings.
"""
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
import onnx
from onnx import TensorProto, helper


def make_model(dim: int, vocab_size: int, out_path: Path) -> None:
    batch = "batch"
    seq = "seq"

    input_ids = helper.make_tensor_value_info("input_ids", TensorProto.INT64, [batch, seq])
    attention_mask = helper.make_tensor_value_info(
        "attention_mask", TensorProto.INT64, [batch, seq]
    )
    embeddings = helper.make_tensor_value_info("embeddings", TensorProto.FLOAT, [batch, dim])

    # Deterministic embedding table: [vocab_size, dim]
    rng = np.random.default_rng(42)
    table = rng.normal(size=(vocab_size, dim)).astype(np.float32)
    table_init = helper.make_tensor(
        "embedding_table", TensorProto.FLOAT, [vocab_size, dim], table.flatten().tolist()
    )

    vocab_size_init = helper.make_tensor(
        "vocab_size", TensorProto.INT64, [], [vocab_size]
    )

    # Build the graph.
    nodes = [
        # Keep token ids inside the table range.
        helper.make_node("Mod", ["input_ids", "vocab_size"], ["mod_ids"], fmod=0),
        # Lookup embeddings.
        helper.make_node("Gather", ["embedding_table", "mod_ids"], ["gathered"], axis=0),
        # Cast attention mask to float and expand to [batch, seq, 1].
        helper.make_node("Cast", ["attention_mask"], ["mask_float"], to=TensorProto.FLOAT),
        helper.make_node("Unsqueeze", ["mask_float", "unsqueeze_axes"], ["mask_3d"]),
        # Zero out padded positions.
        helper.make_node("Mul", ["gathered", "mask_3d"], ["masked"]),
        # Sum over sequence length.
        helper.make_node("ReduceSum", ["masked", "reduce_axes"], ["sum_embeddings"], keepdims=0),
        helper.make_node("ReduceSum", ["mask_3d", "reduce_axes"], ["sum_mask"], keepdims=0),
        # Mean pooling.
        helper.make_node("Div", ["sum_embeddings", "sum_mask"], ["embeddings"]),
    ]

    # Static scalar/1D tensors.
    unsqueeze_axes = helper.make_tensor("unsqueeze_axes", TensorProto.INT64, [1], [2])
    reduce_axes = helper.make_tensor("reduce_axes", TensorProto.INT64, [1], [1])

    graph = helper.make_graph(
        nodes,
        "tiny_embedding_model",
        [input_ids, attention_mask],
        [embeddings],
        [
            table_init,
            vocab_size_init,
            unsqueeze_axes,
            reduce_axes,
        ],
    )

    opset = helper.make_opsetid("", 14)
    model = helper.make_model(graph, opset_imports=[opset])
    model.ir_version = 8
    onnx.checker.check_model(model)
    onnx.save(model, out_path)


def main() -> None:
    dim = 8
    vocab_size = 256
    out_path = Path(__file__).with_name("test_embedder.onnx")
    make_model(dim, vocab_size, out_path)

    # Emit a manifest snippet the Rust tests can embed.
    data = out_path.read_bytes()
    checksum = hashlib.sha256(data).hexdigest()
    manifest = {
        "tiny-test-embedder": {
            "url": "file://localhost" + str(out_path.resolve()),
            "sha256": checksum,
            "dim": dim,
        }
    }
    manifest_path = out_path.with_suffix(".json")
    manifest_path.write_text(json.dumps(manifest, indent=2))
    print(f"Wrote {out_path} ({len(data)} bytes)")
    print(f"SHA-256: {checksum}")
    print(f"Manifest: {manifest_path}")


if __name__ == "__main__":
    main()
