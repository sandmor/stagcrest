# Agent guide

Pointers for AI agents working in this repo. **Do not load every doc upfront** —
open only what matches your task.

## Rendering documentation

**Full reference:** [`crates/stagcrest-render/docs/RENDERING.md`](crates/stagcrest-render/docs/RENDERING.md)

## Conventions for agents

- World time and sun direction math live in `stagcrest-protocol` — do not
  duplicate axis conventions in client code without checking §5.5 of the rendering doc.
- Graphics toggles flow: `data/settings.toml` → `stagcrest-content` →
  `GraphicsSettings` resource → `graphics.rs` camera components.
