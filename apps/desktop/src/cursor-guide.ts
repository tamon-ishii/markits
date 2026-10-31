type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
interface GuideStatus { shortcut: string; window_title: string; error: string | null }

export function showCursorGuide(invoke: Invoke) {
  document.body.innerHTML = `<main class="cursor-guide">
    <h1>マウスカーソル付き撮影</h1>
    <p id="cursor-guide-target" class="cursor-guide-target"></p>
    <p>カーソルを撮りたい位置へ動かしてください。</p>
    <p class="cursor-guide-key"><kbd id="cursor-guide-key">準備中…</kbd><span>キーをもう一度押して撮影してください。</span></p>
    <p class="cursor-guide-footer">Esc で取消 <button id="cursor-guide-cancel" class="btn">キャンセル</button></p>
    <p id="cursor-guide-error" role="alert" hidden></p>
  </main>`;
  const error = document.getElementById('cursor-guide-error')!;
  const cancel = document.getElementById('cursor-guide-cancel') as HTMLButtonElement;
  const renderStatus = (status: GuideStatus) => {
    cancel.disabled = false;
    document.getElementById('cursor-guide-key')!.textContent = status.shortcut.replace('Control', 'Ctrl').replaceAll('+', ' + ');
    const target = document.getElementById('cursor-guide-target')!;
    target.textContent = `対象：${status.window_title}`;
    target.title = status.window_title;
    error.hidden = !status.error;
    error.textContent = status.error ? `撮影できませんでした：${status.error}。${status.shortcut.replace('Control', 'Ctrl')}キーをもう一度押して再試行できます。` : '';
  };
  const refreshStatus = async () => {
    try { renderStatus(await invoke<GuideStatus>('cmd_cursor_guide_status')); }
    catch (cause) { error.hidden = false; error.textContent = String(cause); }
  };
  cancel.onclick = async () => {
    cancel.disabled = true;
    try { await invoke('cmd_cancel_capture'); }
    catch (cause) { error.hidden = false; error.textContent = String(cause); cancel.disabled = false; }
  };
  void (async () => {
    try {
      const { listen } = await import('@tauri-apps/api/event');
      await listen<GuideStatus>('cursor-guide-update', event => renderStatus(event.payload));
      document.addEventListener('visibilitychange', () => { if (!document.hidden) void refreshStatus(); });
      await refreshStatus();
    } catch (cause) { error.hidden = false; error.textContent = String(cause); }
  })();
}
