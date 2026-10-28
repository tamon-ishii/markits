type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
interface RecordedEvent { kind: string; value?: string; x?: number; y?: number; dx?: number; dy?: number; elapsed_ms: number }
interface Status { running: boolean; paused: boolean; events: RecordedEvent[] }

export function addRecordingButton(invoke: Invoke) {
  const button = document.createElement('button');
  button.id = 'btn-record-operations';
  button.className = 'btn';
  button.textContent = 'ウインドウ操作を記録';
  document.querySelector('.action-bar')?.prepend(button);
  button.addEventListener('click', () => { void chooseRecordingWindow(invoke); });
}

async function chooseRecordingWindow(invoke: Invoke) {
  const dialog = document.createElement('dialog');
  dialog.innerHTML = `<h2>ウインドウ操作を記録</h2><p>クリック・文字入力・キー・スクロールを操作データとして保存します。</p>
    <label for="record-window">対象ウインドウ</label><select id="record-window" class="form-control"></select>
    <p role="status"></p><button data-action="refresh" class="btn">一覧を更新</button>
    <button data-action="start" class="btn btn-primary">記録を開始</button><button data-action="cancel" class="btn">キャンセル</button>`;
  document.body.append(dialog);
  dialog.showModal();
  const select = dialog.querySelector('select')!;
  const status = dialog.querySelector('[role=status]')!;
  const start = dialog.querySelector<HTMLButtonElement>('[data-action=start]')!;
  const refresh = dialog.querySelector<HTMLButtonElement>('[data-action=refresh]')!;
  const cancel = dialog.querySelector<HTMLButtonElement>('[data-action=cancel]')!;
  let starting = false;
  dialog.addEventListener('cancel', event => { if (starting) event.preventDefault(); });
  const reload = async () => {
    start.disabled = true; refresh.disabled = true;
    try {
      const windows = await invoke<Array<{id: string; title: string}>>('cmd_recording_windows');
      select.replaceChildren(...windows.map(window => new Option(window.title, window.id)));
      status.textContent = windows.length ? '' : '対象ウインドウがありません。';
    } catch (error) { status.textContent = String(error); }
    finally { start.disabled = !select.options.length; refresh.disabled = false; }
  };
  refresh.onclick = () => { void reload(); };
  cancel.onclick = () => dialog.close();
  dialog.onclose = () => dialog.remove();
  start.onclick = async () => {
    starting = true; start.disabled = true; refresh.disabled = true; cancel.disabled = true;
    status.textContent = '記録を準備中…';
    try { await invoke('cmd_recording_start', { windowId: select.value }); dialog.close(); }
    catch (error) { status.textContent = String(error); }
    finally { starting = false; start.disabled = false; refresh.disabled = false; cancel.disabled = false; }
  };
  await reload();
}

export function showRecordingControls(invoke: Invoke) {
  document.body.innerHTML = `<main class="recording-panel"><h2>ウインドウ操作の記録</h2>
    <p id="record-status" role="status">記録中</p><p>Ctrl＋Shift＋F9：一時停止／再開<br>Ctrl＋Shift＋F10：入力記録を停止</p>
    <button id="record-screenshot" class="btn">📷 スクリーンショットを追加</button>
    <div id="record-screenshots" class="recording-screenshots" aria-label="撮影したスクリーンショット"></div>
    <label><input id="record-export-json" type="checkbox"> 操作記録のJSONも保存</label>
    <button id="record-pause" class="btn">一時停止</button><button id="record-finish" class="btn btn-primary">終了して画像を保存</button>
    <button id="record-cancel" class="btn">取消</button><ol id="record-events"></ol></main>`;
  const pause = document.getElementById('record-pause') as HTMLButtonElement;
  const finish = document.getElementById('record-finish') as HTMLButtonElement;
  const cancel = document.getElementById('record-cancel') as HTMLButtonElement;
  const screenshot = document.getElementById('record-screenshot') as HTMLButtonElement;
  const thumbnails = document.getElementById('record-screenshots')!;
  const status = document.getElementById('record-status')!;
  const list = document.getElementById('record-events')!;
  let paused = false, busy = false, polling = false;
  let completed: unknown;
  screenshot.onclick = async () => {
    if (busy || completed) return;
    busy = true; screenshot.disabled = true; finish.disabled = true; cancel.disabled = true; pause.disabled = true;
    try {
      const url = await invoke<string>('cmd_recording_screenshot');
      const image = document.createElement('img');
      image.src = url; image.alt = `スクリーンショット ${thumbnails.children.length + 1}`;
      thumbnails.append(image);
      status.textContent = `${thumbnails.children.length} 枚の撮影ポイントを追加しました`;
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
      list.replaceChildren(...result.events.slice(-100).map(event => {
        const item = document.createElement('li');
        item.textContent = `${(event.elapsed_ms / 1000).toFixed(1)}秒 ${event.kind} ${event.value ?? (event.x === undefined ? '' : `(${Math.round(event.x)}, ${Math.round(event.y ?? 0)})`)}`;
        return item;
      }));
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
