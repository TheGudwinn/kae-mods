# Third-party notices

This project is licensed under the GNU General Public License v3.0 (see [LICENSE](LICENSE)). The following
bodies of third-party source are copied into this repository.

## Deadlock Mod Manager (3D preview rendering)

- **Upstream:** [deadlock-mod-manager/deadlock-mod-manager](https://github.com/deadlock-mod-manager/deadlock-mod-manager)
- **License:** GNU General Public License v3.0
- **Scope:** [src/features/skins/render/](src/features/skins/render/), adapted from
  `apps/desktop/src/lib/animated-materials.ts`, `apps/desktop/src/lib/material-shader-hooks.ts` and
  `apps/desktop/src/components/foundry/npr-outline.ts`. The viewer's lighting values, tone mapping and
  post-processing settings in [src/features/skins/HeroViewer.tsx](src/features/skins/HeroViewer.tsx) follow
  `viewer-scene.ts` and `viewer-postfx.ts` of the same project.
- **Changes:** validation with `zod` replaced by plain checks; imports adjusted.

## source2-model

- **Scope:** [src-tauri/source2-model/](src-tauri/source2-model/), see
  [LICENSE-ValveResourceFormat](src-tauri/source2-model/LICENSE-ValveResourceFormat) for the notice it carries.
