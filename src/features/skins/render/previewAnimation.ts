// Adapted from Deadlock Mod Manager (https://github.com/deadlock-mod-manager/deadlock-mod-manager), GPL-3.0.
// Source: apps/desktop/src/lib/preview-animation.ts (`createPreviewClip`)
// Changes: the generated type is declared here instead of imported.

import { AnimationClip, Object3D, PropertyBinding, QuaternionKeyframeTrack, VectorKeyframeTrack } from "three";

/** The bone tracks of one animation on one timeline, as `hero_animation` decodes them. */
export interface PreviewAnimation {
  duration: number;
  times: number[];
  tracks: { name: string; positions: number[]; rotations: number[]; scales: number[] }[];
}

/** A three.js clip that drives the bones of `root` by name; bones the model does not have are skipped. */
export function createPreviewClip(root: Object3D, animation: PreviewAnimation): AnimationClip {
  const tracks = animation.tracks.flatMap((track) => {
    const bone = root.getObjectByName(PropertyBinding.sanitizeNodeName(track.name));
    if (!bone) return [];
    return [
      new VectorKeyframeTrack(`${bone.uuid}.position`, animation.times, track.positions),
      new QuaternionKeyframeTrack(`${bone.uuid}.quaternion`, animation.times, track.rotations),
      new VectorKeyframeTrack(`${bone.uuid}.scale`, animation.times, track.scales),
    ];
  });
  return new AnimationClip("preview", animation.duration, tracks);
}
