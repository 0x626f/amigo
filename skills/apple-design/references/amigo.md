# Apple design in Amigo

Amigo is a native Rust/egui application on Linux. Apply the macOS desktop guidance
from this skill to its hierarchy, density, keyboard support and pointer behavior.
The app uses egui controls and its existing vector icons and bundled font.

Use shared components in `crates/ui/src/design/controls.rs` and `dialogs.rs`:

- Ordinary buttons use soft fills and no decorative outline; toolbar icon buttons
  rest quietly and reveal a background on hover. Keep a visible press state and
  keyboard focus ring. Hover animation must preserve the hit target.
- Checkboxes use small rounded squares, neutral empty outlines, and blue selected fills with visible white ticks. Keep their labels clickable, keyboard focus visible, and disabled controls dimmed. Use the shared regular and compact helpers rather than circular dot indicators.
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

## Shared desktop design

- Keep neutral surfaces distinct from selected states: `blue_tint` identifies a
  selection, `hover` gives pointer feedback, and `field` identifies an editable
  surface. Inactive navigation rows inherit their container's background.
- Use `design::PAGE_TITLE_SIZE` for page headings. Keep document typography and
  financial summary values independent from the interface heading scale.
- Navigation rows expose their selected state to accessibility tools and retain
  a visible keyboard focus ring. Muted labels and semantic status colors must
  remain legible against the supported light and dark surfaces.
- Popovers use the shared menu margins, rounded corners, and popup shadow;
  dialogs use the shared modal shadow. Group commands with captions and
  separators rather than turning every option into a prominent button.
- Settings distinguish navigation categories with inset rules and related preferences with rounded groups. Reuse `settings_group`, `settings_row`, and `settings_separator`; keep Save/Cancel outside scrolling content and preserve keyboard and drag behavior.
- Preserve explicit user layout preferences, including the compact Journal
  action icons, their trailing alignment, and setup transfers in App Settings.

## Review of the shared design refactor

These verdicts cover the shared palette, navigation, typography, menus, and
Journal surfaces changed in this refactor, rather than certifying the entire
application against every platform requirement.

| Principle | Verdict | Evidence |
| --- | --- | --- |
| Purpose | Conforms | Content remains prominent; the shared title scale leaves room for commands. |
| Agency | Conforms | Existing keyboard, draft, cancellation, and undo behavior passes its tests. |
| Responsibility | Conforms | The refactor changes presentation; previews use isolated synthetic libraries. |
| Familiarity | Conforms | Navigation and primary actions share blue emphasis; inactive rows inherit their container. |
| Flexibility | Conforms | Contrast checks pass for both palettes; layouts were checked at 980 × 640 and 1440 × 900. |
| Simplicity | Conforms | Quiet inactive rows and grouped menus reduce competing visual emphasis. |
| Craft | Conforms | Page titles share a size token; dialogs and popovers share theme-aware elevation. |
| Delight | Conforms | Existing interruptible transitions and stationary hit targets remain intact. |

Verification: workspace tests and Clippy; native Journal, settings, Tasks, and
Finance previews in both themes and window sizes. The palette regression test
checks normal interface text and semantic colors against a 4.5:1 contrast target.
