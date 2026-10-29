type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
interface PlaybackProgress {
  running: boolean;
  step: number;
  total: number;
  message: string;
  completed: string[];
  captures: number;
  error: string | null;
}

export function showPlaybackControls(invoke: Invoke) {
  document.body.innerHTML = `<main class="playback-panel">
    <h2>操作を再生して再撮影</h2>
    <p id="playback-count">準備中…</p>
    <progress id="playback-progress" max="1" value="0" aria-label="再生の進捗"></progress>
    <p id="playback-current" role="status">対象ウィンドウを確認中…</p>
    <p id="playback-captures"></p>
    <p id="playback-error" role="alert" hidden></p>
    <button id="playback-stop" class="btn btn-danger">停止</button>
    <h3>実行済みの操作</h3><ol id="playback-completed"></ol>
  </main>`;
  const count = document.getElementById('playback-count')!;
  const current = document.getElementById('playback-current')!;
  const captures = document.getElementById('playback-captures')!;
  const error = document.getElementById('playback-error')!;
  const bar = document.getElementById('playback-progress') as HTMLProgressElement;
  const stop = document.getElementById('playback-stop') as HTMLButtonElement;
  const list = document.getElementById('playback-completed')!;
  let running = true;
  let stopping = false;
  stop.onclick = async () => {
    stop.disabled = true;
    try {
      if (running) {
        await invoke('cmd_playback_cancel');
        stopping = true;
        stop.textContent = '停止中…';
      } else {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        await getCurrentWindow().close();
      }
    } catch (cause) {
      error.hidden = false; error.textContent = String(cause); stop.disabled = false;
    }
  };
  const poll = async () => {
    try {
      const status = await invoke<PlaybackProgress>('cmd_playback_status');
      running = status.running;
      count.textContent = `${status.step} / ${status.total} 件`;
      current.textContent = status.message;
      captures.textContent = `新しく撮影した画像：${status.captures} 枚`;
      bar.max = Math.max(1, status.total);
      bar.value = status.completed.length;
      error.hidden = !status.error;
      error.textContent = status.error ?? '';
      for (let index = list.children.length; index < status.completed.length; index++) {
        const item = document.createElement('li');
        item.textContent = `✓ ${status.completed[index]}`;
        list.append(item);
      }
      if (!running) { stop.disabled = false; stop.textContent = '閉じる'; return; }
      stop.disabled = stopping;
    } catch (cause) {
      error.hidden = false; error.textContent = `進捗を取得できませんでした：${cause}`;
    }
    window.setTimeout(() => { void poll(); }, 200);
  };
  void poll();
}
