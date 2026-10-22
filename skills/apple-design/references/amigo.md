# Apple design in Amigo

Amigo is a native Rust/egui application on Linux. Apply the macOS desktop guidance
from this skill to its hierarchy, density, keyboard support and pointer behavior.
The app uses egui controls and its existing vector icons and bundled font.

Use shared components in `crates/ui/src/design/controls.rs` and `dialogs.rs`:

- Ordinary buttons use soft fills and no decorative outline; toolbar icon buttons
  rest quietly and reveal a background on hover. Keep a visible press state and
  keyboard focus ring. Hover animation must preserve the hit target.
- Give a dialog one prominent blue confirmation action at the trailing edge.
  Keep Cancel and secondary actions visually quieter. Destructive actions keep
  their confirmation flow.
- Rounded controls, readable labels and spacing establish hierarchy. Use borders
  for meaningful content separation rather than outlining every action.
- Preserve the existing input sizes, table alignment, scrolling, dark theme and
  keyboard contracts documented by the Amigo UI and behavior skills.
- Direct manipulation has a discoverable handle, a drop-position indicator and
  an alternative control. In board settings, dragging statuses changes a draft;
  Save Board confirms it, and cancellation preserves stored order and assignments.

These are Amigo implementation choices informed by Apple HIG, rather than a claim
that egui reproduces Apple's native materials or uses Apple's platform point sizes.
Use official HIG links in the references for specific Apple requirements.
