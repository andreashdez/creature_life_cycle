# Simulation sprites

The insect and leaf artwork was created with the built-in imagegen tool, then exported as **32 × 32 RGBA PNGs** with ImageMagick's point (nearest-neighbor) filter. The exports preserve the generated alpha channel. The insects face up and the leaf points diagonally upward. All use chunky pixel silhouettes, dark outlines and pale edge accents to stay readable on the board without a background disc.

- `aphid.png`: chartreuse pear-shaped body, antennae and six legs.
- `ladybug.png`: coral shell, six dark spots, centre seam, cream cheek patches and six legs.
- `leaf.png`: pixel-art green leaf with a short stem, pale veins and bright edge accents, used at 32 logical pixels in the food summary card.

The Bevy renderer embeds the PNGs in the executable and shares one texture per species across the board, edit previews and UI icons. The insect and leaf textures explicitly use `ImageSampler::nearest()`. Keep these assets at their native 32 × 32 resolution and avoid bilinear resizing when exporting variants.

Explicit world size includes transparent padding. Movement/birth tweens, cell slots, crowding scale and count badges still apply. The art is static, without a walk-cycle atlas. At distant zoom, the existing species markers take over once cells are smaller than 28 logical pixels.

## Generation prompts

### Aphid

Use case: stylized-concept. Asset type: production pixel-art sprite for a tiny ecology simulation. Create ONE top-down lime-green aphid facing straight up on genuinely transparent alpha. Design on a strict 32 by 32 logical pixel grid, enlarged only by nearest-neighbor, so ALL pixel blocks are equal square size and aligned. Sprite occupies central 28 by 28 logical pixels. Pear-shaped lime abdomen, small green head, two short antennae and six stout legs. Simple 6-color palette: near-black forest outline, deep green, medium green, lime, pale yellow-green highlight and small cream rim accents. Thick continuous dark outline hugs the insect silhouette, plus selective bright edge pixels so it reads over dark olive-green board cells. Flat solid square pixel clusters, broad color areas, VERY minimal detail, clean 1990s handheld-game pixel art. No smooth curves, no antialiasing, no gradients, no texture, no tiny rendered detail, no soft lighting. All background pixels fully transparent, all painted pixels fully opaque. No circle behind it, no disc, no cast shadow, no ground, no text, no checkerboard. Only one aphid.

### Ladybug

Use case: stylized-concept. Asset type: production pixel-art sprite for a tiny ecology simulation. Create ONE top-down coral-red ladybug facing straight up on genuinely transparent alpha. Design on a strict 32 by 32 logical pixel grid, enlarged only by nearest-neighbor, so ALL pixel blocks are equal square size and aligned. Sprite occupies central 28 by 28 logical pixels. Round coral-red wing cases, bold dark central seam, six large black spots, small charcoal head with two cream cheek pixels, two short antennae and six stout legs. Simple 6-color palette: near-black warm outline, dark brick red, red, coral, peach highlight and cream rim accents. Thick continuous dark outline hugs insect silhouette, plus selective bright edge pixels so it reads over dark olive-green board cells. Flat solid square pixel clusters, broad color areas, VERY minimal detail, clean 1990s handheld-game pixel art. No smooth curves, no antialiasing, no gradients, no texture, no tiny rendered detail, no soft lighting. All background pixels fully transparent, all painted pixels fully opaque. No circle behind it, no disc, no cast shadow, no ground, no text, no checkerboard. Only one ladybug.

### Leaf

Use case: stylized-concept. Asset type: production pixel-art food icon for the sidebar of a tiny ecology simulation, matching chunky chartreuse aphid and coral ladybug pixel sprites. Create ONE fresh green leaf with a short stem, diagonally rising from lower left to upper right, on genuinely transparent alpha. Design on a strict 32 by 32 logical pixel grid, enlarged only by nearest-neighbor, so ALL pixel blocks are equal square size and aligned. Leaf and stem occupy central 26 by 26 logical pixels. Simple green leaf silhouette, pale yellow-green central vein and just two short side veins. Simple 6-color palette: near-black forest outline, deep green, medium green, lime, pale yellow-green highlight and small cream rim accents. Thick continuous dark outline hugs the leaf silhouette, plus selective bright edge pixels so it reads over a dark charcoal sidebar. Flat solid square pixel clusters, broad color areas, VERY minimal detail, clean 1990s handheld-game pixel art. No smooth curves, no antialiasing, no gradients, no texture, no tiny rendered detail, no soft lighting. All background pixels fully transparent, all painted pixels fully opaque. No circle behind it, no disc, no cast shadow, no ground, no text, no checkerboard. Only one isolated leaf with transparent margins.
