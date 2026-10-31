type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
interface RecordedEvent {
  kind: string; value?: string; x?: number; y?: number; dx?: number; dy?: number; elapsed_ms: number;
  target?: { name: string; role: string } | null;
  visual?: { png: string; anchor_x: number; anchor_y: number } | null;
  include_cursor?: boolean;
  pointer?: { x: number; y: number; target?: { name: string } | null } | null;
}
interface Status { running: boolean; paused: boolean; events: RecordedEvent[] }

export function addRecordingButton(invoke: Invoke, currentImage?: () => string | null) {
  const button = document.createElement('button');
  button.id = 'btn-record-operations';
  button.className = 'btn';
  button.textContent = '⏺ 操作を記録';
  document.querySelector('.action-bar')?.prepend(button);
  button.addEventListener('click', async () => {
    button.disabled = true;
    try {
      const imageDataUrl = currentImage?.();
      if (imageDataUrl) await invoke('cmd_recording_start_from_image', { imageDataUrl });
      else await chooseRecordingWindow(invoke);
    } catch (error) { alert(String(error)); }
    finally { button.disabled = false; }
  });
}

async function chooseRecordingWindow(invoke: Invoke) {
  try {
    await invoke('cmd_select_recording_window');
  } catch (error) {
    alert(String(error));
  }
}

export function showRecordingControls(invoke: Invoke) {
  document.body.innerHTML = `<main class="recording-panel"><h2>操作を記録</h2>
    <p>操作は下のリストに順次追加されます。撮影キーならマウスを動かさず、ホバー中の画面も撮影できます。</p>
    <p class="record-capture-shortcut"><span>撮影キー</span> <kbd id="record-capture-key">準備中…</kbd><span>マウスを動かさず撮影</span></p>
    <p>対象ウィンドウの操作だけを記録します。クリックは「名称で照合」または「座標のみ」で記録されます。</p>
    <p id="record-status" role="status">記録中</p><p>Ctrl＋Shift＋F9：一時停止／再開<br>Ctrl＋Shift＋F10：入力記録を停止</p>
    <button id="record-screenshot" class="btn btn-primary">📷 キャプチャ</button>
    <label id="record-cursor-label" hidden><input id="record-include-cursor" type="checkbox" disabled> ポインタ付き撮影</label>
    <div id="record-screenshots" class="recording-screenshots" aria-label="撮影したスクリーンショット"></div>
    <label><input id="record-export-json" type="checkbox"> 操作記録のJSONも保存</label>
    <button id="record-pause" class="btn">一時停止</button><button id="record-finish" class="btn btn-primary">終了して画像を保存</button>
    <button id="record-cancel" class="btn">取消</button><h3>操作リスト</h3><ol id="record-events" aria-label="録画した操作"></ol></main>`;
  const pause = document.getElementById('record-pause') as HTMLButtonElement;
  const finish = document.getElementById('record-finish') as HTMLButtonElement;
  const cancel = document.getElementById('record-cancel') as HTMLButtonElement;
  const screenshot = document.getElementById('record-screenshot') as HTMLButtonElement;
  const thumbnails = document.getElementById('record-screenshots')!;
  const status = document.getElementById('record-status')!;
  const list = document.getElementById('record-events')!;
  const cursor = document.getElementById('record-include-cursor') as HTMLInputElement;
  cursor.onchange = async () => {
    cursor.disabled = true;
    try { cursor.checked = await invoke<boolean>('cmd_recording_cursor_mode', { enabled: cursor.checked }); }
    catch (error) { cursor.checked = !cursor.checked; status.textContent = String(error); }
    finally { cursor.disabled = false; }
  };
  let paused = false, busy = false, polling = false;
  let completed: unknown;
  const addScreenshot = (url: string) => {
    const image = document.createElement('img');
    image.src = url; image.alt = `スクリーンショット ${thumbnails.children.length + 1}`;
    thumbnails.append(image);
    status.textContent = `${thumbnails.children.length} 枚の撮影ポイントを追加しました`;
  };
  screenshot.disabled = true;
  void (async () => {
    try {
      const { listen } = await import('@tauri-apps/api/event');
      await listen<string>('recording-screenshot', event => addScreenshot(event.payload));
      await listen<string>('recording-capture-error', event => { status.textContent = event.payload; });
      const shortcut = await invoke<{ shortcut: string }>('cmd_get_capture_shortcut');
      if (await invoke<boolean>('cmd_cursor_capture_supported')) {
        document.getElementById('record-cursor-label')!.hidden = false;
        cursor.checked = await invoke<boolean>('cmd_recording_cursor_mode');
        cursor.disabled = false;
      }
      document.getElementById('record-capture-key')!.textContent = shortcut.shortcut.replace('Control', 'Ctrl').replaceAll('+', ' + ');
      screenshot.disabled = false;
    } catch (error) { status.textContent = String(error); }
  })();
  screenshot.onclick = async () => {
    if (busy || completed) return;
    busy = true; screenshot.disabled = true; finish.disabled = true; cancel.disabled = true; pause.disabled = true;
    try {
      await invoke<string>('cmd_recording_screenshot');
    } catch (error) { status.textContent = String(error); }
    finally { busy = false; screenshot.disabled = false; finish.disabled = false; cancel.disabled = false; pause.disabled = false; }
  };
  const poll = async () => {
    if (busy || completed || polling) return;
    polling = true;
    try {
      const result = await invoke<Status>('cmd_recording_status');
      if (busy || completed) return;
      paused = result.paused;
      pause.textContent = paused ? '再開' : '一時停止';
      pause.disabled = !result.running;
      status.textContent = `${result.running ? paused ? '一時停止中' : '記録中' : '入力記録は停止済み・保存できます'}：${result.events.length} 件`;
      const labels: Record<string, string> = { click: 'クリック', text: '文字入力', key: 'キー', scroll: 'スクロール', screenshot: '📷 キャプチャ' };
      result.events.forEach((event, index) => {
        const item = list.children[index] ?? list.appendChild(document.createElement('li'));
        const position = event.x === undefined ? '' : `(${Math.round(event.x)}, ${Math.round(event.y ?? 0)})`;
        const detail = event.kind === 'screenshot' && event.include_cursor
          ? `ポインタ付き${event.pointer?.target ? `／ホバー：${event.pointer.target.name}` : event.pointer ? `／位置：(${Math.round(event.pointer.x)}, ${Math.round(event.pointer.y)})` : ''}`
          : event.kind === 'click'
          ? event.target
            ? `【名称で照合】「${event.target.name}」／種類：${event.target.role}／記録位置：${position}`
            : `【座標のみ】名称を取得できませんでした／記録位置：${position}`
          : event.kind === 'scroll'
          ? `横 ${event.dx ?? 0} / 縦 ${event.dy ?? 0}`
          : event.value ?? position;
        const text = `${(event.elapsed_ms / 1000).toFixed(1)}秒 ${labels[event.kind] ?? event.kind} ${detail}`;
        let label = item.querySelector('span');
        if (!label) { label = document.createElement('span'); item.appendChild(label); }
        if (label.textContent !== text) label.textContent = text;

      });
      while (list.children.length > result.events.length) list.lastElementChild?.remove();
    } catch (error) { if (!busy && !completed) status.textContent = String(error); }
    finally { polling = false; }
  };
  pause.onclick = async () => {
    pause.disabled = true;
    try { await invoke('cmd_recording_pause', { paused: !paused }); await poll(); }
    catch (error) { status.textContent = String(error); }
    finally { pause.disabled = false; }
  };
  finish.onclick = async () => {
    if (busy) return;
    if (!thumbnails.children.length) {
      status.textContent = '保存する画像を「キャプチャ」ボタンで撮影してください。';
      return;
    }
    busy = true; finish.disabled = true; pause.disabled = true; cancel.disabled = true; screenshot.disabled = true;
    try {
      completed ??= await invoke('cmd_recording_finish');
      if ((document.getElementById('record-export-json') as HTMLInputElement).checked) {
        const { save } = await import('@tauri-apps/plugin-dialog');
        const path = await save({ defaultPath: 'markits-operations.json', filters: [{ name: 'Operation recording', extensions: ['json'] }] });
        if (!path) { status.textContent = '画像は保存済みです。JSONの保存先を選び直せます。'; return; }
        const { writeFile } = await import('@tauri-apps/plugin-fs');
        await writeFile(path, new TextEncoder().encode(JSON.stringify(completed, null, 2)));
      }
      clearInterval(timer);
      await invoke('cmd_recording_cancel');
    } catch (error) { status.textContent = String(error); }
    finally { busy = false; finish.disabled = false; cancel.disabled = false; pause.disabled = Boolean(completed); screenshot.disabled = Boolean(completed); }
  };
  cancel.onclick = async () => {
    if (busy) return;
    busy = true;
    try { await invoke('cmd_recording_cancel'); clearInterval(timer); }
    catch (error) { status.textContent = String(error); busy = false; }
  };
  document.addEventListener('keydown', event => { if (event.key === 'Escape') cancel.click(); });
  const timer = setInterval(() => { void poll(); }, 750);
  void poll();
}
