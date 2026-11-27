import init, { WebPlayer } from "/pkg/radeon_8500.js";

const slug = document.body.dataset.demo;
const byId = (name) => document.getElementById(name);

start().catch(showError);

function showError(error) {
  const status = byId("status");
  status.textContent = error instanceof Error ? error.message : String(error);
  status.hidden = false;
}

async function download(config) {
  const parts = await Promise.all(config.parts.map(async (url) => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Content request failed (${response.status}).`);
    return new Uint8Array(await response.arrayBuffer());
  }));
  const content = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    content.set(part, offset);
    offset += part.length;
  }
  return content;
}

async function start() {
  const canvas = byId("canvas");
  const stage = byId("stage");
  const status = byId("status");
  const notice = byId("notice");
  function notify(message) {
    notice.textContent = message;
    notice.hidden = false;
  }
  const geo = slug === "geoforms";
  const [, config] = await Promise.all([
    init(),
    fetch(`/content/${slug}.json`).then((response) => {
      if (!response.ok) throw new Error("Original content is unavailable.");
      return response.json();
    }),
  ]);
  const content = await download(config);
  status.textContent = "Preparing the GPU…";
  function canvasSize() {
    const width = Math.max(1, stage.clientWidth);
    const height = Math.max(1, stage.clientHeight);
    // Keep the full viewport while bounding fragment work on Retina displays.
    const pixelBudget = geo ? 720 * 720 : 1920 * 1080;
    const resolution = Math.min(devicePixelRatio, 1.5, Math.sqrt(pixelBudget / (width * height)));
    return [Math.max(1, Math.round(width * resolution)), Math.max(1, Math.round(height * resolution))];
  }
  [canvas.width, canvas.height] = canvasSize();
  const player = await WebPlayer.create(canvas, slug, content);
  const pause = byId("pause");
  const restart = byId("restart");
  const sound = byId("sound");
  let playing = true;
  let audio;
  let soundtrackIndex = 0;
  let audioUrl;
  let analyser;
  let frequency;
  let audioContext;

  function togglePause() {
    playing = !playing;
    pause.textContent = playing ? "Pause" : "Resume";
    if (audio && !audio.muted) {
      if (playing) audio.play().catch(() => {});
      else audio.pause();
    }
  }

  pause.onclick = () => { player.key(" "); togglePause(); };
  function reset() {
    player.key("r");
    byId("truform").setAttribute("aria-pressed", "true");
    byId("chest-object").value = "0";
    playing = true;
    pause.textContent = "Pause";
    if (audio && !geo) {
      soundtrackIndex = 0;
      audio.src = config.tracks[0];
      audio.loop = config.tracks.length === 1 && slug === "rachel";
    } else if (audio) {
      audio.currentTime = 0;
    }
    if (audio && !audio.muted) audio.play().catch(() => {});
  }
  restart.onclick = reset;
  const truform = byId("truform");
  if (slug === "dolphin") {
    truform.hidden = false;
    truform.onclick = () => {
      player.key("t");
      truform.setAttribute("aria-pressed", truform.getAttribute("aria-pressed") === "false");
    };
  }
  if (slug === "treasure-chest") {
    byId("chest-select").hidden = false;
    byId("chest-object").onchange = (event) => player.key(event.target.value);
  }
  pause.disabled = false;
  restart.disabled = false;

  if (config.tracks.length) {
    audio = new Audio(config.tracks[0]);
    audio.preload = "none";
    audio.loop = config.tracks.length === 1 && slug === "rachel";
    audio.muted = true;
    audio.addEventListener("ended", () => {
      if (soundtrackIndex + 1 >= config.tracks.length) return;
      soundtrackIndex++;
      audio.src = config.tracks[soundtrackIndex];
      audio.loop = true;
      if (playing && !audio.muted) audio.play().catch(() => {});
    });
    sound.hidden = false;
    sound.onclick = () => {
      audio.muted = !audio.muted;
      sound.textContent = audio.muted ? "Unmute" : "Mute";
      if (!audio.muted && playing) audio.play().catch(() => {});
      else audio.pause();
    };
  }

  if (geo) {
    const file = byId("audio-file");
    byId("load-audio").hidden = false;
    byId("preset").hidden = false;
    byId("preset").onclick = () => player.key("m");
    byId("stencil").hidden = false;
    byId("stencil").onclick = () => player.key("s");
    file.onchange = async () => {
      if (!file.files[0]) return;
      if (!audio) {
        audio = new Audio();
        audio.loop = true;
        audioContext = new AudioContext();
        analyser = audioContext.createAnalyser();
        analyser.fftSize = 1024;
        frequency = new Uint8Array(analyser.frequencyBinCount);
        audioContext.createMediaElementSource(audio).connect(analyser).connect(audioContext.destination);
      }
      notice.hidden = true;
      if (audioUrl) URL.revokeObjectURL(audioUrl);
      audioUrl = URL.createObjectURL(file.files[0]);
      audio.src = audioUrl;
      try {
        await audioContext.resume();
        if (playing) await audio.play();
      } catch {
        notify("This audio file could not be played.");
      }
    };
  }

  const fullscreen = async () => {
    if (!document.fullscreenEnabled) return;
    notice.hidden = true;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await byId("viewer").requestFullscreen();
    } catch {
      notify("Fullscreen is unavailable.");
    }
  };
  const fullscreenButton = byId("fullscreen");
  fullscreenButton.hidden = !document.fullscreenEnabled;
  fullscreenButton.disabled = false;
  fullscreenButton.onclick = fullscreen;
  document.addEventListener("fullscreenchange", () => {
    fullscreenButton.textContent = document.fullscreenElement ? "Exit fullscreen" : "Fullscreen";
  });
  byId("keyboard").textContent = geo
    ? "Space pauses · R restarts · M changes deformation · S changes stencil"
    : "Drag to look · Scroll to zoom · Space pauses · Arrow keys seek · R restarts"
      + (slug === "dolphin" ? " · T toggles TruForm" : "")
      + (slug === "treasure-chest" ? " · 0–7 select a showpiece" : "");
  document.addEventListener("keydown", (event) => {
    if (event.target.matches("button,input,select,summary") || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key.toLowerCase() === "f") {
      if (document.fullscreenEnabled) event.preventDefault();
      fullscreen();
      return;
    }
    if (event.key.toLowerCase() === "r") reset();
    else if (slug === "dolphin" && event.key.toLowerCase() === "t") truform.click();
    else if (!player.key(event.key)) return;
    if (event.key === " ") togglePause();
    if (slug === "treasure-chest" && /^[0-7]$/.test(event.key)) byId("chest-object").value = event.key;
    event.preventDefault();
  });
  if (!geo) {
    canvas.addEventListener("pointerdown", (event) => canvas.setPointerCapture(event.pointerId));
    canvas.addEventListener("pointermove", (event) => {
      if (canvas.hasPointerCapture(event.pointerId)) player.rotate(event.movementX, event.movementY);
    });
    canvas.addEventListener("wheel", (event) => {
      event.preventDefault();
      player.zoom(Math.sign(event.deltaY));
    }, { passive: false });
  }

  new ResizeObserver(() => {
    const [width, height] = canvasSize();
    if (canvas.width !== width || canvas.height !== height) {
      [canvas.width, canvas.height] = [width, height];
      player.resize(width, height);
    }
  }).observe(stage);

  let level = 0;
  function frame(now) {
    if (analyser && !audio.paused) {
      analyser.getByteFrequencyData(frequency);
      const bottom = frequency.subarray(1, 12);
      const signal = bottom.reduce((sum, value) => sum + value, 0) / (255 * bottom.length);
      level = Math.max(signal, level * 0.92);
      player.audio(level);
    }
    try {
      if (player.frame(now / 1000)) {
        status.hidden = true;
        stage.dataset.ready = "true";
      }
      requestAnimationFrame(frame);
    } catch (error) {
      showError(error);
    }
  }
  requestAnimationFrame(frame);
}
