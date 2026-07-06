import { writable } from 'svelte/store';

export type LodMode = 'auto' | 'manual';
export type NaturePreviewProfile = 'authoring' | 'mobile' | 'console';

interface EditorState {
  selectedNode: string | null;
  showWireframe: boolean;
  showNormals: boolean;
  showScaleReference: boolean;
  autoRotate: boolean;
  autoRotateSpeed: number;
  lodMode: LodMode;
  currentLod: number;
  naturePreviewLod: number;
  naturePreviewProfile: NaturePreviewProfile;
  autoRegenerate: boolean;
  windEnabled: boolean;
  windStrength: number;
  windSpeed: number;
  windDirection: number;
  sunAzimuth: number;
  sunElevation: number;
  sunIntensity: number;
}

function createEditorStore() {
  const { subscribe, set, update } = writable<EditorState>({
    selectedNode: null,
    showWireframe: false,
    showNormals: false,
    showScaleReference: false,
    autoRotate: false,
    autoRotateSpeed: 1.0,
    lodMode: 'manual',
    currentLod: 0,
    naturePreviewLod: 0,
    naturePreviewProfile: 'authoring',
    autoRegenerate: true,
    windEnabled: true,
    windStrength: 1.0,
    windSpeed: 1.5,
    windDirection: 20,
    sunAzimuth: 45,
    sunElevation: 55,
    sunIntensity: 1.5
  });

  return {
    subscribe,
    selectNode: (id: string | null) => update(s => ({ ...s, selectedNode: id })),
    toggleWireframe: () => update(s => ({ ...s, showWireframe: !s.showWireframe })),
    toggleNormals: () => update(s => ({ ...s, showNormals: !s.showNormals })),
    toggleScaleReference: () => update(s => ({ ...s, showScaleReference: !s.showScaleReference })),
    toggleAutoRotate: () => update(s => ({ ...s, autoRotate: !s.autoRotate })),
    setAutoRotateSpeed: (speed: number) => update(s => ({ ...s, autoRotateSpeed: speed })),
    setLodMode: (lodMode: LodMode) => update(s => ({ ...s, lodMode })),
    setLod: (lod: number) => update(s => ({ ...s, currentLod: lod })),
    setNaturePreviewLod: (naturePreviewLod: number) => update(s => ({ ...s, naturePreviewLod })),
    setNaturePreviewProfile: (naturePreviewProfile: NaturePreviewProfile) => update(s => ({ ...s, naturePreviewProfile })),
    toggleAutoRegenerate: () => update(s => ({ ...s, autoRegenerate: !s.autoRegenerate })),
    toggleWindEnabled: () => update(s => ({ ...s, windEnabled: !s.windEnabled })),
    setWindStrength: (windStrength: number) => update(s => ({ ...s, windStrength })),
    setWindSpeed: (windSpeed: number) => update(s => ({ ...s, windSpeed })),
    setWindDirection: (windDirection: number) => update(s => ({ ...s, windDirection })),
    setSunAzimuth: (sunAzimuth: number) => update(s => ({ ...s, sunAzimuth })),
    setSunElevation: (sunElevation: number) => update(s => ({ ...s, sunElevation })),
    setSunIntensity: (sunIntensity: number) => update(s => ({ ...s, sunIntensity })),
    reset: () => set({
      selectedNode: null,
      showWireframe: false,
      showNormals: false,
      showScaleReference: false,
      autoRotate: false,
      autoRotateSpeed: 1.0,
      lodMode: 'manual',
      currentLod: 0,
      naturePreviewLod: 0,
      naturePreviewProfile: 'authoring',
      autoRegenerate: true,
      windEnabled: true,
      windStrength: 1.0,
      windSpeed: 1.5,
      windDirection: 20,
      sunAzimuth: 45,
      sunElevation: 55,
      sunIntensity: 1.5
    })
  };
}

export const editorStore = createEditorStore();
