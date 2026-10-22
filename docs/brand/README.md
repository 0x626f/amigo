# Amigo logo

A geometric lowercase “a” on a flat cobalt tile, paired with a lowercase wordmark.
The circular shape and consistent stroke stay readable in the sidebar and desktop launcher.
The palette is cobalt (`#2463EB`), white, and charcoal (`#20242C`).

| Asset | Use |
| --- | --- |
| [App icon SVG](amigo-icon.svg) · [1024px PNG](amigo-icon.png) | Desktop icon, launchers, and sharing |
| [Transparent mark](amigo-mark.svg) | Places where a tile background is unnecessary |
| [Light wordmark SVG](amigo-logo.svg) · [PNG](amigo-logo.png) | Light backgrounds |
| [Dark wordmark SVG](amigo-logo-dark.svg) · [PNG](amigo-logo-dark.png) | Dark backgrounds |

The wordmarks use outlined Adwaita Sans, so no installed font is required to display them.
The font's license is included in [the bundled fonts](../../crates/ui/assets/fonts/LICENSE).
Keep the icon square, retain its transparent corners, and leave space around the logo.

The app uses `packaging/amigo.png` (256px) for its native window and cached sidebar texture.
Linux packages install both `packaging/amigo.svg` and `packaging/amigo.png`.
After editing `amigo-icon.svg`, update those copies with ImageMagick:

```sh
cp docs/brand/amigo-icon.svg packaging/amigo.svg
magick -background none docs/brand/amigo-icon.svg -resize 256x256 -depth 8 packaging/amigo.png
magick -background none -density 384 docs/brand/amigo-icon.svg -resize 1024x1024 -depth 8 docs/brand/amigo-icon.png
```
