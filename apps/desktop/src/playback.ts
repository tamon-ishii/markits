type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
interface PlaybackProgress {
  running: boolean;
  paused: boolean;
  wait_total_ms: number;
  wait_remaining_ms: number;
  step: number;
  total: number;
  message: string;
  completed: string[];
  operations: string[];
  waits_ms: number[];
  captures: number;
  error: string | null;
  target_image: string | null;
  target_description: string | null;
}

export function showPlaybackControls(invoke: Invoke) {
  document.body.innerHTML = `<main class="playback-panel">
    <header class="playback-heading"><span class="playback-eyebrow">CAPTURE PLAYBACK</span><h2>再撮影</h2><p>記録した操作を再生して、新しい画像を撮影します。</p></header>
    <section class="playback-summary">
    <p id="playback-count">準備中…</p>
    <progress id="playback-progress" max="1" value="0" aria-label="再生の進捗"></progress>
    <p id="playback-current" role="status">対象ウィンドウを確認中…</p>
    <div class="playback-wait"><div><span id="playback-wait-label">待機時間</span><strong id="playback-wait-time">—</strong></div><progress id="playback-wait-bar" max="1" value="0" aria-label="残り待機時間"></progress></div>
    <p id="playback-target" hidden></p>
    <img id="playback-target-image" alt="記録時のクリック対象の周辺画像" hidden style="max-width:100%;image-rendering:pixelated;border:1px solid currentColor">
    <p id="playback-captures"></p>
    <p id="playback-error" role="alert" hidden></p>
    </section>
    <div class="playback-controls">
    <button id="playback-pause" class="btn">一時停止</button>
    <button id="playback-resume" class="btn btn-primary">▶ 再生</button>
    <button id="playback-step" class="btn">次のステップ</button>
    <button id="playback-stop" class="btn btn-danger">停止</button>
    </div>
    <h3>操作リスト <span>待機時間を編集できます</span></h3><ol id="playback-completed"></ol>
  </main>`;
  const count = document.getElementById('playback-count')!;
  const current = document.getElementById('playback-current')!;
  const captures = document.getElementById('playback-captures')!;
  const target = document.getElementById('playback-target')!;
  const targetImage = document.getElementById('playback-target-image') as HTMLImageElement;
  const error = document.getElementById('playback-error')!;
  const bar = document.getElementById('playback-progress') as HTMLProgressElement;
  const stop = document.getElementById('playback-stop') as HTMLButtonElement;
  const pause = document.getElementById('playback-pause') as HTMLButtonElement;
  const resume = document.getElementById('playback-resume') as HTMLButtonElement;
  const step = document.getElementById('playback-step') as HTMLButtonElement;
  for (const [button, action] of [[pause, 'pause'], [resume, 'resume'], [step, 'step']] as const) {
    button.onclick = async () => {
      try {
        if (action === 'resume' && !running) {
          const { emitTo } = await import('@tauri-apps/api/event');
          await emitTo('main', 'restart-playback');
        } else await invoke('cmd_playback_control', { action });
      }
      catch (cause) { error.hidden = false; error.textContent = String(cause); }
    };
  }
  const list = document.getElementById('playback-completed')!;
  let running = true;
  let stopping = false;
  let previousStep = -1;
  stop.onclick = async () => {
    if (!running) return;
    stop.disabled = true;
    try {
      if (running) {
        await invoke('cmd_playback_cancel');
        stopping = true;
        stop.textContent = '停止中…';
      }
    } catch (cause) {
      error.hidden = false; error.textContent = String(cause); stop.disabled = false;
    }
  };
  const poll = async () => {
    try {
      const status = await invoke<PlaybackProgress>('cmd_playback_status');
      running = status.running;
      const remaining = running ? status.wait_remaining_ms : 0;
      const waitBar = document.getElementById('playback-wait-bar') as HTMLProgressElement;
      waitBar.max = Math.max(1, status.wait_total_ms);
      waitBar.value = remaining;
      document.getElementById('playback-wait-time')!.textContent = remaining > 0 ? `${(remaining / 1000).toFixed(1)} 秒` : '—';
      document.getElementById('playback-wait-label')!.textContent = remaining > 0 ? status.paused ? '一時停止中 · 残り' : '次の操作まで' : running ? '再生の準備' : '待機終了';
      count.textContent = `${status.step} / ${status.total} 件`;
      current.textContent = status.message;
      target.hidden = !status.target_description;
      target.textContent = status.target_description ?? '';
      targetImage.hidden = !status.target_image;
      if (status.target_image && targetImage.getAttribute('src') !== status.target_image) {
        targetImage.src = status.target_image;
      }
      captures.textContent = `新しく撮影した画像：${status.captures} 枚`;
      bar.max = Math.max(1, status.total);
      bar.value = status.completed.length;
      error.hidden = !status.error;
      error.textContent = status.error ?? '';
      status.operations.forEach((operation, index) => {
        const item = (list.children[index] ?? list.appendChild(document.createElement('li'))) as HTMLElement;
        const done = index < status.completed.length;
        const active = index === status.step - 1 && !done;
        if (!item.firstElementChild) {
          const label = document.createElement('label');
          label.hidden = index === 0;
          label.textContent = '待機（ミリ秒） ';
          const input = document.createElement('input');
          input.type = 'number'; input.min = '0'; input.max = '3600000'; input.step = '100';
          input.value = String(status.waits_ms[index]);
          input.setAttribute('aria-label', `操作 ${index + 1} の前の待機時間（ミリ秒）`);
          input.onchange = async () => {
            const milliseconds = Math.max(0, Math.min(3600000, Math.round(Number(input.value) || 0)));
            input.value = String(milliseconds);
            try { await invoke('cmd_playback_capture_wait', { index, milliseconds }); }
            catch (cause) { error.hidden = false; error.textContent = String(cause); }
          };
          label.append(input);
          const text = document.createElement('div');
          item.append(label, text);
        }
        (item.lastElementChild as HTMLElement).textContent = `${done ? '✓' : active ? '▶' : '○'} ${operation}`;
        (item.querySelector('input') as HTMLInputElement).disabled = done;
        item.classList.toggle('playback-active', active);
        item.classList.toggle('playback-done', done);
        if (active) item.setAttribute('aria-current', 'step'); else item.removeAttribute('aria-current');
        if (active && previousStep !== status.step) item.scrollIntoView({ block: 'nearest' });
      });
      while (list.children.length > status.operations.length) list.lastElementChild?.remove();
      previousStep = status.step;
      await invoke('cmd_playback_ready');
      if (!running) stopping = false;
      stop.textContent = stopping ? '停止中…' : '停止';
      pause.disabled = !running || status.paused || stopping;
      resume.disabled = running && (!status.paused || stopping);
      step.disabled = !running || !status.paused || stopping;
      stop.disabled = !running || stopping;
    } catch (cause) {
      error.hidden = false; error.textContent = `進捗を取得できませんでした：${cause}`;
    }
    window.setTimeout(() => { void poll(); }, 200);
  };
  void poll();
}
