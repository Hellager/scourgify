import { useEffect, useRef, useState } from "react";

const SWEEP_DURATION_MS = 2_600;
const BEATS_PER_SWEEP = 3.25;
const SAMPLES_PER_PIXEL = 2;
const GAP_WIDTH_PX = 4;
const MAX_PIXEL_RATIO = 2;

const ECG_KEYFRAMES = [
  [0, 0],
  [0.24, 0],
  [0.28, -1],
  [0.34, 1],
  [0.38, -0.4],
  [0.42, 0],
  [1, 0],
] as const;

export function AutoCleanMonitorPulse({ label }: { label: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [reduceMotion, setReduceMotion] = useState(false);

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = () => setReduceMotion(mediaQuery.matches);
    updatePreference();
    mediaQuery.addEventListener("change", updatePreference);
    return () => mediaQuery.removeEventListener("change", updatePreference);
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }

    let animationFrame = 0;
    let logicalWidth = 0;
    let logicalHeight = 0;
    let pixelRatio = 1;
    let samples = new Float32Array(0);
    let samplesPerBeat = 1;
    let sampleInterval = 16;
    let totalSamples = 0;
    let writeIndex = 0;
    let accumulator = 0;
    let previousTime = 0;

    const drawRange = (start: number, end: number) => {
      if (end - start < 1) {
        return;
      }

      context.beginPath();
      for (let index = start; index <= end; index += 1) {
        const x = (index / (samples.length - 1)) * logicalWidth;
        const y = logicalHeight / 2 + samples[index] * logicalHeight * 0.31;
        if (index === start) {
          context.moveTo(x, y);
        } else {
          context.lineTo(x, y);
        }
      }
      context.stroke();
    };

    const draw = (sweeping: boolean) => {
      if (samples.length === 0 || logicalWidth === 0 || logicalHeight === 0) {
        return;
      }

      context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
      context.clearRect(0, 0, logicalWidth, logicalHeight);
      context.strokeStyle = getComputedStyle(canvas).color;
      context.lineCap = "round";
      context.lineJoin = "round";
      context.lineWidth = 1.15;
      context.globalAlpha = 0.88;

      if (!sweeping) {
        drawRange(0, samples.length - 1);
        return;
      }

      const gapPoints = Math.max(
        3,
        Math.round((GAP_WIDTH_PX / logicalWidth) * samples.length),
      );
      const gapEnd = writeIndex + gapPoints;
      if (gapEnd < samples.length) {
        drawRange(0, writeIndex - 1);
        drawRange(gapEnd, samples.length - 1);
      } else {
        drawRange(gapEnd - samples.length, writeIndex - 1);
      }

      const headIndex = (writeIndex - 1 + samples.length) % samples.length;
      const headX = (headIndex / (samples.length - 1)) * logicalWidth;
      const headY =
        logicalHeight / 2 + samples[headIndex] * logicalHeight * 0.31;
      context.globalAlpha = 1;
      context.fillStyle = getComputedStyle(canvas).color;
      context.beginPath();
      context.arc(headX, headY, 1.35, 0, Math.PI * 2);
      context.fill();
    };

    const resizeCanvas = () => {
      const bounds = canvas.getBoundingClientRect();
      if (bounds.width === 0 || bounds.height === 0) {
        return;
      }

      logicalWidth = bounds.width;
      logicalHeight = bounds.height;
      pixelRatio = Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO);
      canvas.width = Math.round(logicalWidth * pixelRatio);
      canvas.height = Math.round(logicalHeight * pixelRatio);

      const sampleCount = Math.max(
        64,
        Math.round(logicalWidth * SAMPLES_PER_PIXEL),
      );
      samplesPerBeat = sampleCount / BEATS_PER_SWEEP;
      samples = Float32Array.from({ length: sampleCount }, (_, index) =>
        sampleEcg(index, samplesPerBeat),
      );
      sampleInterval = SWEEP_DURATION_MS / sampleCount;
      totalSamples = sampleCount;
      writeIndex = 0;
      accumulator = 0;
      previousTime = 0;
      draw(!reduceMotion);
    };

    const animate = (time: number) => {
      if (samples.length === 0) {
        animationFrame = window.requestAnimationFrame(animate);
        return;
      }

      if (previousTime === 0) {
        previousTime = time;
      }
      accumulator += Math.min(time - previousTime, 100);
      previousTime = time;

      while (accumulator >= sampleInterval) {
        samples[writeIndex] = sampleEcg(totalSamples, samplesPerBeat);
        writeIndex = (writeIndex + 1) % samples.length;
        totalSamples += 1;
        accumulator -= sampleInterval;
      }

      draw(true);
      animationFrame = window.requestAnimationFrame(animate);
    };

    const resizeObserver = new ResizeObserver(resizeCanvas);
    const themeObserver = new MutationObserver(() => draw(!reduceMotion));
    resizeObserver.observe(canvas);
    themeObserver.observe(document.documentElement, {
      attributeFilter: ["class"],
      attributes: true,
    });
    resizeCanvas();

    if (!reduceMotion) {
      animationFrame = window.requestAnimationFrame(animate);
    }

    return () => {
      window.cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      themeObserver.disconnect();
    };
  }, [reduceMotion]);

  return (
    <div className="relative h-9 w-20 max-w-full overflow-hidden text-foreground">
      <canvas
        aria-hidden="true"
        className="block h-full w-full"
        ref={canvasRef}
      />
      <span className="sr-only">{label}</span>
    </div>
  );
}

function sampleEcg(sampleIndex: number, samplesPerBeat: number) {
  const phase = (sampleIndex % samplesPerBeat) / samplesPerBeat;

  for (let index = 1; index < ECG_KEYFRAMES.length; index += 1) {
    const [nextPhase, nextValue] = ECG_KEYFRAMES[index];
    if (phase <= nextPhase) {
      const [previousPhase, previousValue] = ECG_KEYFRAMES[index - 1];
      const progress = (phase - previousPhase) / (nextPhase - previousPhase);
      return previousValue + (nextValue - previousValue) * progress;
    }
  }

  return 0;
}
