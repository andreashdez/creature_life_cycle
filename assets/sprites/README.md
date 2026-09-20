# Bug sprites

Created with the built-in imagegen tool. Both PNGs are 1254 × 1254 RGBA with transparent backgrounds, centred, facing up. Keep their alpha channel when exporting variants.

- `aphid.png`: chartreuse pear-shaped abdomen, long antennae, six legs and paired cornicles.
- `ladybug.png`: coral round shell, dark spots, centre seam, ivory cheek patches and six legs.

The Bevy renderer embeds the PNGs in the executable, shares one texture per species and sets an explicit world size, including transparent padding. Direct and packaged launches do not need an external sprite directory. Existing movement/birth tweens, cell slots, crowding scale and count badges still apply. The art is intentionally static; there is no walk-cycle atlas. At very distant zoom levels the silhouette and colour carry identification.

## Generation prompts

### Aphid

Use case: stylized-concept. Asset type: production 2D game sprite for a Bevy ecology simulation, one aphid, square 1024x1024 PNG with genuinely transparent alpha background. Orthographic straight top-down view, head at 12 o'clock, centered with all appendages within central 84% of canvas. A charming but clearly insect-like lime-green aphid: pear-shaped broad abdomen tapering to a small head, six short thick splayed legs, two curved antennae, subtle paired cornicles near rear, two tiny dark eyes. Bold readable silhouette, dark forest-green outline, light chartreuse shell with a few broad soft cel-shaded shapes and one restrained pale highlight. Clean polished illustrated game art with smooth antialiased edges. This must read clearly when reduced to 20–40 pixels; simplify details, use thick appendages, no texture noise, no hair, no elaborate facial expression. Mostly bilateral symmetry. No ground, no shadow outside the insect, no leaf, no scene, no text, no border, no checkerboard background. Only a single isolated aphid sprite, generous transparent margin.

### Ladybug

Use case: stylized-concept. Asset type: production 2D game sprite for a Bevy ecology simulation, one ladybug, square PNG with genuinely transparent alpha background. Orthographic straight top-down view, head at 12 o'clock, centered with all appendages within central 84% of canvas. A charming but clearly insect-like coral-red ladybug: round domed wing cases, strong dark center seam, exactly seven large near-black spots (three paired spots plus one central spot at front), small charcoal head with two ivory cheek patches, six short thick splayed charcoal legs, two short curved antennae. Bold readable silhouette, near-black warm outline, coral vermilion shell with broad soft cel-shaded shapes and one restrained warm pale highlight. Clean polished illustrated game art with smooth antialiased edges. This must read clearly when reduced to 20–40 pixels; simplify details, use thick appendages and large spots, no texture noise, no hair, no elaborate facial expression. Mostly bilateral symmetry. No ground, no shadow outside the insect, no leaf, no scene, no text, no border, no checkerboard background. Only a single isolated ladybug sprite, generous transparent margin.
