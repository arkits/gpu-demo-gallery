import init, { WebPlayer } from "/pkg/radeon_8500.js";

const demos = {
  dolphin: ["ATI Dolphin", "ATI Radeon 8500 · 2001"],
  nature: ["ATI Nature", "ATI Radeon 8500 · 2001"],
  rachel: ["ATI Rachel", "ATI Radeon 8500 · 2001"],
  island1: ["ATI Island 1", "ATI Radeon 8500 · 2001"],
  island2: ["ATI Island 2", "ATI Radeon 8500 · 2001"],
  island3: ["ATI Island 3", "ATI Radeon 8500 · 2001"],
  "treasure-chest": ["ATI Treasure Chest", "ATI Radeon 8500 · 2001"],
  geoforms: ["GeoForms", "NVIDIA GeForce 7 · 2006"],
};

const slug = location.pathname.split("/")[1];
const byId = (name) => document.getElementById(name);
for (const [id, [title]] of Object.entries(demos)) {
  const link = Object.assign(document.createElement("a"), { href: `/${id}/`, textContent: title });
  if (id === slug) link.setAttribute("aria-current", "page");
  byId("demos").append(link);
}

if (Object.hasOwn(demos, slug)) {
  const [title, hardware] = demos[slug];
  document.title = `${title} · GPU Demo Gallery`;
  byId("title").textContent = title;
  byId("hardware").textContent = hardware;
  byId("viewer").hidden = false;
  start().catch((error) => {
    byId("status").textContent = error instanceof Error ? error.message : String(error);
  });
} else {
  byId("directory").hidden = false;
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
    const resolution = Math.min(devicePixelRatio, 1.5);
    return [Math.max(1, Math.round(stage.clientWidth * resolution)),
      Math.max(1, Math.round(stage.clientHeight * resolution))];
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
      if (audioUrl) URL.revokeObjectURL(audioUrl);
      audioUrl = URL.createObjectURL(file.files[0]);
      audio.src = audioUrl;
      try {
        await audioContext.resume();
        if (playing) await audio.play();
      } catch (error) {
        byId("keyboard").textContent = "This audio file could not be played. Try MP3 or WAV.";
      }
    };
  }

  const fullscreen = async () => {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await stage.requestFullscreen();
  };
  byId("fullscreen").onclick = () => fullscreen().catch(() => {});
  byId("keyboard").textContent = geo
    ? "Space pauses · R restarts · M changes deformation · S changes stencil"
    : "Drag to look · Scroll to zoom · Space pauses · Arrow keys seek · R restarts"
      + (slug === "dolphin" ? " · T toggles TruForm" : "");
  document.addEventListener("keydown", (event) => {
    if (event.target.matches("button,input,select") || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key.toLowerCase() === "f") {
      fullscreen().catch(() => {});
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
    if (player.frame(now / 1000)) {
      status.hidden = true;
      stage.dataset.ready = "true";
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}
