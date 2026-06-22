function hasWebGL2() {
  try {
    const canvas = document.createElement("canvas");
    return !!(
      canvas.getContext("webgl2") || canvas.getContext("experimental-webgl2")
    );
  } catch {
    return false;
  }
}

/**
 * Probe WebGPU the same way Bevy/wgpu does: adapter must support a canvas
 * surface, and we must be able to create a device and configure the context.
 * Chrome on Linux often exposes navigator.gpu and even returns an adapter,
 * but fails once a surface is required.
 */
async function probeWebGPU() {
  if (!navigator.gpu) {
    return false;
  }

  let device = null;
  try {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("webgpu");
    if (!context) {
      return false;
    }

    const adapter = await navigator.gpu.requestAdapter({
      compatibleSurface: context,
    });
    if (!adapter) {
      return false;
    }

    device = await adapter.requestDevice();
    if (!device) {
      return false;
    }

    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device, format });

    return true;
  } catch {
    return false;
  } finally {
    device?.destroy();
  }
}

function disableNavigatorGpu() {
  try {
    delete navigator.gpu;
  } catch {
    // ignore
  }
  try {
    Object.defineProperty(navigator, "gpu", {
      value: undefined,
      configurable: true,
    });
  } catch {
    // ignore
  }
}

function showGraphicsError() {
  const overlay = document.getElementById("error-overlay");
  if (overlay) {
    overlay.style.display = "flex";
  }
}

async function preflightGraphics() {
  const webgpuOk = await probeWebGPU();
  globalThis.__STAGCREST_WEBGPU_OK = webgpuOk;

  if (!webgpuOk) {
    disableNavigatorGpu();
  }

  if (!webgpuOk && !hasWebGL2()) {
    showGraphicsError();
    throw new Error("WebGPU and WebGL2 are unavailable");
  }
}

await preflightGraphics();

export default function stagcrestInitializer() {
  return {};
}
