import { AnnotationEditor } from './editor.ts';
import { CaptureOverlay, CropRect } from './overlay.ts';
import { CapturedImage, LoadedImageResult, DetectedUiElement } from './types.ts';

interface HistoryItem {
  id: string;
  timestamp: number;
  date_formatted: string;
  width: number;
  height: number;
  file_path: string;
  thumbnail_data_url: string;
  has_annotations: boolean;
}

// Detect whether running inside Tauri desktop shell
const isTauri = typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);

async function invokeTauri<T>(cmd: string, args: Record<string, any> = {}): Promise<T> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  } else {
    console.warn(`[Mock] invoke called outside Tauri: ${cmd}`, args);
    throw new Error('Tauri API is not available in standalone browser mode');
  }
}

async function raiseAppWindow() {
  if (isTauri) {
    try {
      await invokeTauri('cmd_raise_window');
    } catch (e) {
      console.warn('Failed to raise window:', e);
    }
  }
}

document.addEventListener('DOMContentLoaded', async () => {
  let windowLabel = 'main';
  if (isTauri) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      windowLabel = getCurrentWindow().label;
    } catch (e) {
      console.warn('Failed to get current window label:', e);
    }
  }

  // Dedicated Fullscreen Snipping Overlay Mode
  if (windowLabel === 'overlay') {
    document.body.classList.add('overlay-mode');
    const overlay = new CaptureOverlay();

    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event');
      await listen<CapturedImage>('show-capture-overlay', (event) => {
        const captured = event.payload;
        overlay.show(
          captured.data_url,
          captured.ui_elements || [],
          async (crop: CropRect | null, elements: DetectedUiElement[]) => {
            try {
              await invokeTauri('cmd_finish_capture', {
                rawDataUrl: captured.data_url,
                uiElements: elements,
                cropRect: crop
                  ? {
                      x: crop.x,
                      y: crop.y,
                      width: crop.width,
                      height: crop.height,
                    }
                  : null,
              });
            } catch (err) {
              console.warn('Failed to finish capture:', err);
              await invokeTauri('cmd_cancel_capture').catch((restoreErr) => {
                console.warn('Failed to restore editor after capture error:', restoreErr);
              });
              const { emitTo } = await import('@tauri-apps/api/event');
              await emitTo('main', 'capture-error', String(err));
            }
          },
          async () => {
            try {
              await invokeTauri('cmd_cancel_capture');
            } catch (err) {
              console.warn('Failed to cancel capture:', err);
            }
          }
        );
      });
      await invokeTauri('cmd_overlay_ready');
    }
    return; // Overlay window does not run editor / history logic
  }

  // Main Editor Window Mode
  // Elements
  const tabHome = document.getElementById('tab-home')!;
  const tabEditor = document.getElementById('tab-editor')!;
  const homeView = document.getElementById('home-view')!;
  const editorView = document.getElementById('editor-view')!;
  const editorActions = document.getElementById('editor-actions')!;
  const historyGrid = document.getElementById('history-grid')!;
  const historyCount = document.getElementById('history-count')!;

  let currentHistoryId: string | null = null;

  const autoSaveToHistory = async () => {
    if (!isTauri || !editor.hasImage()) return;
    // Skip saving if already recorded in history and no changes have been made
    if (currentHistoryId && !editor.isDirty()) return;

    try {
      const bgDataUrl = editor.getBackgroundImageDataUrl();
      const sceneJson = editor.getSceneJson();
      const uiElements = editor.getUiElements();
      const item = await invokeTauri<HistoryItem>('cmd_save_to_history', {
        backgroundDataUrl: bgDataUrl,
        sceneJson,
        existingId: currentHistoryId,
        uiElements,
      });
      currentHistoryId = item.id;
      editor.markClean();
    } catch (e) {
      console.warn('Auto-save to history failed:', e);
    }
  };

  const switchView = async (view: 'home' | 'editor') => {
    if (view === 'home') {
      // Switch view and tabs immediately for responsive UI
      tabHome.classList.add('active');
      tabEditor.classList.remove('active');
      homeView.style.display = 'block';
      editorView.style.display = 'none';
      editorActions.style.display = 'none';

      if (editor.hasImage()) {
        await autoSaveToHistory();
      }
      await loadHistory();
    } else {
      tabHome.classList.remove('active');
      tabEditor.classList.add('active');
      homeView.style.display = 'none';
      editorView.style.display = 'flex';
      editorActions.style.display = 'flex';
      await raiseAppWindow();
    }
  };

  tabHome.addEventListener('click', () => switchView('home'));
  tabEditor.addEventListener('click', () => switchView('editor'));

  // Initialize Editor
  const editor = new AnnotationEditor(async (sceneJson: string) => {
    if (isTauri) {
      return await invokeTauri<string>('cmd_render_svg', { sceneJson });
    } else {
      return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600"><text x="20" y="40" fill="#3b82f6" font-size="20">MarkIts Preview</text></svg>`;
    }
  });

  const uiStatusEl = document.getElementById('ui-detection-status')!;
  const setUiStatus = (state: 'idle' | 'scanning' | 'ready' | 'failed', label: string) => {
    uiStatusEl.dataset.state = state;
    uiStatusEl.textContent = label;
  };
  window.addEventListener('markits-image-loaded', () => {
    const count = editor.getUiElements().length;
    setUiStatus(count > 0 ? 'ready' : 'idle', count > 0 ? `UI ${count} 要素` : 'UI 検出なし');
  });

  // Tool buttons
  document.querySelectorAll('.tool-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const tool = btn.getAttribute('data-type');
      if (tool) {
        editor.setTool(tool);
      }
    });
  });

  const recentToolsEl = document.getElementById('recent-tools')!;
  const recentToolListEl = document.getElementById('recent-tool-list')!;
  const toolTemplates = new Map<string, { label: string; title: string }>();
  document.querySelectorAll<HTMLButtonElement>('.palette-panel > .tool-list > .tool-btn').forEach((button) => {
    const type = button.dataset.type;
    if (type && type !== 'select') toolTemplates.set(type, { label: button.textContent?.trim() ?? type, title: button.title });
  });
  let recentTools: string[] = [];
  let activeRecentTool = 'select';
  try {
    const saved = JSON.parse(localStorage.getItem('markits-recent-tools') ?? '[]');
    if (Array.isArray(saved)) recentTools = saved.filter((tool): tool is string => typeof tool === 'string' && toolTemplates.has(tool)).slice(0, 3);
  } catch { /* Storage is optional. */ }
  const renderRecentTools = () => {
    recentToolListEl.replaceChildren();
    recentToolsEl.hidden = recentTools.length === 0;
    for (const tool of recentTools) {
      const template = toolTemplates.get(tool)!;
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'tool-btn';
      button.classList.toggle('active', tool === activeRecentTool);
      button.dataset.type = tool;
      button.title = template.title;
      button.textContent = template.label;
      button.addEventListener('click', () => editor.setTool(tool));
      recentToolListEl.appendChild(button);
    }
  };
  renderRecentTools();
  window.addEventListener('markits-tool-changed', (event) => {
    const tool = (event as CustomEvent<string>).detail;
    activeRecentTool = tool;
    if (tool === 'select') { renderRecentTools(); return; }
    if (!toolTemplates.has(tool)) return;
    recentTools = [tool, ...recentTools.filter((item) => item !== tool)].slice(0, 3);
    try { localStorage.setItem('markits-recent-tools', JSON.stringify(recentTools)); } catch { /* Storage is optional. */ }
    renderRecentTools();
  });

  // Undo / Redo
  document.getElementById('btn-undo')?.addEventListener('click', () => editor.undo());
  document.getElementById('btn-redo')?.addEventListener('click', () => editor.redo());

  // Load and Render History in Home View
  const loadHistory = async () => {
    if (!isTauri) {
      historyGrid.innerHTML = `
        <div class="history-empty">
          <div class="history-empty-icon">🖼️</div>
          <p>ブラウザプレビューでは履歴機能はシミュレーションのみとなります。</p>
        </div>
      `;
      historyCount.textContent = '0 件';
      return;
    }

    try {
      historyGrid.innerHTML = '<div class="history-loading">履歴を読み込んでいます...</div>';
      const items = await invokeTauri<HistoryItem[]>('cmd_get_history');
      historyCount.textContent = `${items.length} 件`;

      if (items.length === 0) {
        historyGrid.innerHTML = `
          <div class="history-empty">
            <div class="history-empty-icon">📸</div>
            <h4>まだキャプチャ履歴がありません</h4>
            <p style="margin-top: 6px; font-size: 13px;">「画面をキャプチャ」ボタンまたは PrintScreen キーで撮影してみましょう。</p>
          </div>
        `;
        return;
      }

      historyGrid.innerHTML = '';
      for (const item of items) {
        const card = document.createElement('div');
        card.className = 'history-card';
        card.innerHTML = `
          <div class="history-thumb-wrap">
            <img class="history-thumb" src="${item.thumbnail_data_url}" alt="Capture ${item.id}" />
            <div class="history-reedit-overlay">
              <span class="reedit-tag">✏️ 再編集する</span>
            </div>
          </div>
          <div class="history-info">
            <div class="history-meta">
              <span class="history-date">${item.date_formatted}</span>
              <span class="history-dims">${item.width} × ${item.height}${item.has_annotations ? ' · 注釈あり' : ''}</span>
            </div>
            <button class="btn-delete-history" title="履歴から削除">🗑️</button>
          </div>
        `;

        // Card Click -> Open and re-edit in Editor
        card.addEventListener('click', async () => {
          try {
            const loaded = await invokeTauri<LoadedImageResult>('cmd_load_history_item', { id: item.id });

            editor.setBackgroundImage(
              loaded.image_data_url,
              loaded.width,
              loaded.height,
              loaded.annotations_json,
              loaded.ui_elements
            );
            currentHistoryId = item.id;
            await switchView('editor');
          } catch (e: any) {
            alert(`キャプチャの読み込みに失敗しました: ${e?.message ?? e}`);
          }
        });

        // Delete button
        const delBtn = card.querySelector('.btn-delete-history') as HTMLButtonElement;
        delBtn.addEventListener('click', async (e) => {
          e.stopPropagation();
          if (confirm('このキャプチャ履歴を削除しますか？')) {
            try {
              await invokeTauri('cmd_delete_history_item', { id: item.id });
              await loadHistory();
            } catch (err: any) {
              alert(`削除エラー: ${err?.message ?? err}`);
            }
          }
        });

        historyGrid.appendChild(card);
      }
    } catch (e: any) {
      historyGrid.innerHTML = `<div class="history-empty"><p>履歴の読み込みに失敗しました: ${e?.message ?? e}</p></div>`;
    }
  };

  document.getElementById('btn-refresh-history')?.addEventListener('click', () => loadHistory());

  // Screen Capture Flow
  const handleCaptureTrigger = async () => {
    if (isTauri) {
      try {
        await invokeTauri('cmd_start_capture');
      } catch (e: any) {
        console.warn('Failed to start capture:', e);
        alert(`画面キャプチャの起動に失敗しました: ${e?.message ?? e}`);
      }
    } else {
      alert('画面キャプチャはデスクトップ環境で動作します。');
    }
  };

  document.getElementById('btn-trigger-capture')?.addEventListener('click', handleCaptureTrigger);
  document.getElementById('btn-hero-capture')?.addEventListener('click', handleCaptureTrigger);
  document.getElementById('btn-empty-capture')?.addEventListener('click', handleCaptureTrigger);

  // File Open Flow
  const handleOpenFile = async () => {
    if (isTauri) {
      try {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const selected = await open({
          multiple: false,
          filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg'] }],
        });

        if (selected && typeof selected === 'string') {
          const result = await invokeTauri<LoadedImageResult>('cmd_load_image', { filePath: selected });

          editor.setBackgroundImage(
            result.image_data_url,
            result.width,
            result.height,
            result.annotations_json,
            result.ui_elements
          );
          currentHistoryId = null;
          await switchView('editor');
        }
      } catch (e: any) {
        alert(`ファイル読み込みエラー: ${e?.message ?? e}`);
      }
    } else {
      document.getElementById('file-input')?.click();
    }
  };

  document.getElementById('btn-open-file')?.addEventListener('click', handleOpenFile);
  document.getElementById('btn-hero-open')?.addEventListener('click', handleOpenFile);

  // Done Editing (編集終了) -> Auto-save to history and return to Home view
  document.getElementById('btn-done-editing')?.addEventListener('click', async () => {
    await switchView('home');
  });

  // Browser file input fallback
  document.getElementById('file-input')?.addEventListener('change', (e) => {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (event) => {
        const dataUrl = event.target?.result as string;
        const img = new Image();
        img.onload = async () => {
          editor.setBackgroundImage(dataUrl, img.width, img.height, null);
          currentHistoryId = null;
          await switchView('editor');
        };
        img.src = dataUrl;
      };
      reader.readAsDataURL(file);
    }
  });

  // Drag and Drop files onto canvas or home view
  window.addEventListener('dragover', (e) => e.preventDefault());
  window.addEventListener('drop', (e) => {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (event) => {
        const dataUrl = event.target?.result as string;
        const img = new Image();
        img.onload = async () => {
          editor.setBackgroundImage(dataUrl, img.width, img.height, null);
          currentHistoryId = null;
          await switchView('editor');
        };
        img.src = dataUrl;
      };
      reader.readAsDataURL(file);
    }
  });

  // --- In-Editor Zoom Controls ---
  document.getElementById('btn-zoom-in')?.addEventListener('click', () => editor.zoomIn());
  document.getElementById('btn-zoom-out')?.addEventListener('click', () => editor.zoomOut());
  document.getElementById('btn-zoom-reset')?.addEventListener('click', () => editor.resetZoom());
  document.getElementById('btn-zoom-fit')?.addEventListener('click', () => editor.zoomFit());
  document.getElementById('zoom-label')?.addEventListener('click', () => editor.resetZoom());

  const editorWidthInput = document.getElementById('editor-export-width') as HTMLInputElement | null;
  const editorHeightInput = document.getElementById('editor-export-height') as HTMLInputElement | null;
  const editorAspectLock = document.getElementById('editor-aspect-lock') as HTMLButtonElement | null;
  let editorAspectLocked = true;
  let editorAspectRatio = 1;
  const syncEditorOutputInputs = () => {
    const dimensions = editor.getOutputDimensions();
    editorWidthInput?.setAttribute('value', String(dimensions.width));
    editorHeightInput?.setAttribute('value', String(dimensions.height));
    if (editorWidthInput) editorWidthInput.value = String(dimensions.width);
    if (editorHeightInput) editorHeightInput.value = String(dimensions.height);
    editorAspectRatio = dimensions.width / Math.max(1, dimensions.height);
  };
  window.addEventListener('markits-image-loaded', () => syncEditorOutputInputs());
  editorAspectLock?.addEventListener('click', () => {
    if (!editorAspectLocked) {
      const width = Number.parseInt(editorWidthInput?.value ?? '', 10);
      const height = Number.parseInt(editorHeightInput?.value ?? '', 10);
      if (Number.isFinite(width) && Number.isFinite(height) && height > 0) {
        editorAspectRatio = width / height;
      }
    }
    editorAspectLocked = !editorAspectLocked;
    editorAspectLock.classList.toggle('active', editorAspectLocked);
    editorAspectLock.textContent = editorAspectLocked ? '🔒' : '🔓';
    editorAspectLock.setAttribute('aria-pressed', String(editorAspectLocked));
  });
  editorWidthInput?.addEventListener('input', () => {
    const width = Number.parseInt(editorWidthInput.value, 10);
    if (!Number.isFinite(width) || width < 10) return;
    if (editorAspectLocked && editorHeightInput) {
      editorHeightInput.value = String(Math.max(10, Math.round(width / editorAspectRatio)));
    }
    const height = Number.parseInt(editorHeightInput?.value ?? '', 10);
    if (Number.isFinite(height) && height >= 10) editor.setOutputDimensions(width, height);
  });
  editorHeightInput?.addEventListener('input', () => {
    const height = Number.parseInt(editorHeightInput.value, 10);
    if (!Number.isFinite(height) || height < 10) return;
    if (editorAspectLocked && editorWidthInput) {
      editorWidthInput.value = String(Math.max(10, Math.round(height * editorAspectRatio)));
    }
    const width = Number.parseInt(editorWidthInput?.value ?? '', 10);
    if (Number.isFinite(width) && width >= 10) editor.setOutputDimensions(width, height);
  });

  // --- In-Editor Crop Control ---
  document.getElementById('btn-crop')?.addEventListener('click', () => {
    editor.toggleCropMode();
  });
  document.getElementById('btn-autocrop')?.addEventListener('click', () => {
    editor.cropToAnnotations(32);
  });

  // --- Export Resolution Modal Flow ---
  const modalExport = document.getElementById('modal-export') as HTMLElement | null;
  const exportOrigDims = document.getElementById('export-orig-dims') as HTMLElement | null;
  const exportWidthInput = document.getElementById('export-width') as HTMLInputElement | null;
  const exportHeightInput = document.getElementById('export-height') as HTMLInputElement | null;
  const btnAspectLock = document.getElementById('btn-aspect-lock') as HTMLButtonElement | null;
  const btnCloseExportModal = document.getElementById('btn-close-export-modal');
  const btnModalCancel = document.getElementById('btn-modal-cancel');
  const btnModalCopy = document.getElementById('btn-modal-copy');
  const btnModalSave = document.getElementById('btn-modal-save');

  let isAspectLocked = true;
  let originalWidth = 1920;
  let originalHeight = 1080;
  let aspectRatio = originalWidth / originalHeight;

  const closeExportModal = () => {
    if (modalExport) modalExport.style.display = 'none';
  };

  const openExportModal = () => {
    const bgDataUrl = editor.getBackgroundImageDataUrl();
    if (!bgDataUrl || bgDataUrl.length === 0) {
      alert('保存またはコピーする画像がありません。まずはキャプチャまたは画像を開いてください。');
      return false;
    }

    const scene = editor.getScene();
    originalWidth = scene.canvas.width;
    originalHeight = scene.canvas.height;
    const currentOutput = editor.getOutputDimensions();
    aspectRatio = originalWidth / Math.max(1, originalHeight);

    if (exportOrigDims) {
      exportOrigDims.textContent = `${originalWidth} × ${originalHeight}`;
    }
    if (exportWidthInput) {
      exportWidthInput.value = currentOutput.width.toString();
    }
    if (exportHeightInput) {
      exportHeightInput.value = currentOutput.height.toString();
    }

    isAspectLocked = editorAspectLocked;
    if (btnAspectLock) {
      btnAspectLock.classList.toggle('active', isAspectLocked);
      btnAspectLock.textContent = isAspectLocked ? '🔒' : '🔓';
    }

    // Reset preset chips to original
    document.querySelectorAll('.preset-chip').forEach((chip) => {
      chip.classList.toggle('active', chip.getAttribute('data-preset') === 'original');
    });

    if (modalExport) {
      modalExport.style.display = 'flex';
    }
    return true;
  };

  btnCloseExportModal?.addEventListener('click', closeExportModal);
  btnModalCancel?.addEventListener('click', closeExportModal);

  // Aspect lock toggle
  btnAspectLock?.addEventListener('click', () => {
    isAspectLocked = !isAspectLocked;
    btnAspectLock.classList.toggle('active', isAspectLocked);
    btnAspectLock.textContent = isAspectLocked ? '🔒' : '🔓';
  });

  const applyExportDimensionsToEditor = () => {
    const width = exportWidthInput ? Number.parseInt(exportWidthInput.value, 10) : NaN;
    const height = exportHeightInput ? Number.parseInt(exportHeightInput.value, 10) : NaN;
    if (!Number.isFinite(width) || !Number.isFinite(height) || width < 10 || height < 10) return;
    editor.setOutputDimensions(width, height);
    if (editorWidthInput) editorWidthInput.value = String(width);
    if (editorHeightInput) editorHeightInput.value = String(height);
  };

  // Width input change
  exportWidthInput?.addEventListener('input', () => {
    const w = parseInt(exportWidthInput.value, 10);
    if (!isNaN(w) && w > 0 && isAspectLocked && exportHeightInput) {
      const h = Math.max(1, Math.round(w / aspectRatio));
      exportHeightInput.value = h.toString();
    }
    applyExportDimensionsToEditor();
    document.querySelectorAll('.preset-chip').forEach((chip) => chip.classList.remove('active'));
  });

  // Height input change
  exportHeightInput?.addEventListener('input', () => {
    const h = parseInt(exportHeightInput.value, 10);
    if (!isNaN(h) && h > 0 && isAspectLocked && exportWidthInput) {
      const w = Math.max(1, Math.round(h * aspectRatio));
      exportWidthInput.value = w.toString();
    }
    applyExportDimensionsToEditor();
    document.querySelectorAll('.preset-chip').forEach((chip) => chip.classList.remove('active'));
  });

  // Preset chips
  document.querySelectorAll('.preset-chip').forEach((chip) => {
    chip.addEventListener('click', () => {
      const preset = chip.getAttribute('data-preset');
      let targetW = originalWidth;
      let targetH = originalHeight;

      switch (preset) {
        case 'original':
          targetW = originalWidth;
          targetH = originalHeight;
          break;
        case 'w800':
          targetW = 800;
          targetH = Math.max(1, Math.round(800 / aspectRatio));
          break;
        case 'w1200':
          targetW = 1200;
          targetH = Math.max(1, Math.round(1200 / aspectRatio));
          break;
        case 'w1920':
          targetW = 1920;
          targetH = Math.max(1, Math.round(1920 / aspectRatio));
          break;
        case 'scale-50':
          targetW = Math.max(1, Math.round(originalWidth * 0.5));
          targetH = Math.max(1, Math.round(originalHeight * 0.5));
          break;
        case 'scale-200':
          targetW = Math.max(1, Math.round(originalWidth * 2.0));
          targetH = Math.max(1, Math.round(originalHeight * 2.0));
          break;
      }

      if (exportWidthInput) exportWidthInput.value = targetW.toString();
      if (exportHeightInput) exportHeightInput.value = targetH.toString();
      applyExportDimensionsToEditor();

      document.querySelectorAll('.preset-chip').forEach((c) => c.classList.toggle('active', c === chip));
    });
  });

  const getExportDimensions = (): { width: number | null; height: number | null } => {
    let w = exportWidthInput ? parseInt(exportWidthInput.value, 10) : originalWidth;
    let h = exportHeightInput ? parseInt(exportHeightInput.value, 10) : originalHeight;
    if (isNaN(w) || w <= 0) w = originalWidth;
    if (isNaN(h) || h <= 0) h = originalHeight;
    if (w === originalWidth && h === originalHeight) {
      return { width: null, height: null };
    }
    return { width: w, height: h };
  };

  const executeSaveWithResolution = async () => {
    const bgDataUrl = editor.getBackgroundImageDataUrl();
    if (!bgDataUrl || bgDataUrl.length === 0) return;

    if (!isTauri) {
      alert('ブラウザプレビューモードでは保存APIは動作しません。');
      return;
    }

    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const savePath = await save({
        filters: [{ name: 'PNG Image', extensions: ['png'] }],
        defaultPath: `markits_${Date.now()}.png`,
      });

      if (savePath) {
        const { width, height } = getExportDimensions();
        await invokeTauri('cmd_compose_and_save', {
          backgroundDataUrl: bgDataUrl,
          sceneJson: editor.getSceneJson(),
          savePath,
          uiElements: editor.getUiElements(),
          exportWidth: width,
          exportHeight: height,
        });
        await autoSaveToHistory();
        closeExportModal();
        alert('指定解像度のPNGファイルとして保存しました（再編集メタデータ付き）。');
      }
    } catch (e: any) {
      alert(`保存に失敗しました: ${e?.message ?? e}`);
    }
  };

  const executeCopyWithResolution = async () => {
    const bgDataUrl = editor.getBackgroundImageDataUrl();
    if (!bgDataUrl || bgDataUrl.length === 0) return;

    if (!isTauri) {
      alert('クリップボードAPIはデスクトップ環境で動作します。');
      return;
    }

    try {
      const { width, height } = getExportDimensions();
      await invokeTauri('cmd_copy_to_clipboard', {
        backgroundDataUrl: bgDataUrl,
        sceneJson: editor.getSceneJson(),
        exportWidth: width,
        exportHeight: height,
      });
      await autoSaveToHistory();
      closeExportModal();
      alert('クリップボードに指定解像度で画像をコピーしました！');
    } catch (e: any) {
      alert(`クリップボードへのコピーに失敗しました: ${e?.message ?? e}`);
    }
  };

  // Header "Save" button triggers export modal
  document.getElementById('btn-save-file')?.addEventListener('click', () => {
    openExportModal();
  });

  // Header "Copy" button triggers export modal
  document.getElementById('btn-copy-clipboard')?.addEventListener('click', () => {
    openExportModal();
  });

  // Modal action buttons
  btnModalSave?.addEventListener('click', executeSaveWithResolution);
  btnModalCopy?.addEventListener('click', executeCopyWithResolution);

  // Toggle UI Snap button
  const btnToggleSnap = document.getElementById('btn-toggle-snap') as HTMLButtonElement;
  btnToggleSnap?.addEventListener('click', () => {
    const active = editor.toggleSnap();
    if (active) {
      btnToggleSnap.classList.add('active');
    } else {
      btnToggleSnap.classList.remove('active');
    }
  });

  // Listen to Tauri events from system tray, global shortcuts, or overlay window
  if (isTauri) {
    const { listen } = await import('@tauri-apps/api/event');

    await listen<LoadedImageResult>('capture-finished', async (event) => {
      const result = event.payload;
      editor.setBackgroundImage(
        result.image_data_url,
        result.width,
        result.height,
        result.annotations_json,
        result.ui_elements
      );
      currentHistoryId = result.history_id ?? null;
      await switchView('editor');
    });

    await listen<{ history_id: string; elements: DetectedUiElement[] }>('capture-ui-elements', (event) => {
      if (event.payload.history_id === currentHistoryId) {
        editor.setUiElements(event.payload.elements);
        const count = editor.getUiElements().length;
        setUiStatus(count > 0 ? 'ready' : 'idle', count > 0 ? `UI ${count} 要素` : 'UI 要素なし');
      }
    });

    await listen<{ history_id: string; status: string }>('capture-ui-scan-status', (event) => {
      if (event.payload.history_id !== currentHistoryId) return;
      if (event.payload.status === 'started') setUiStatus('scanning', 'UI 解析中…');
      else if (event.payload.status === 'timeout') setUiStatus('failed', 'UI 取得が時間切れ');
      else if (event.payload.status === 'failed') setUiStatus('failed', 'UI を取得できません');
    });

    await listen<string>('capture-error', (event) => {
      alert(`画面キャプチャに失敗しました: ${event.payload}`);
    });

    await listen('trigger-capture', async () => {
      await raiseAppWindow();
      handleCaptureTrigger();
    });
    await listen('trigger-open-file', async () => {
      await raiseAppWindow();
      handleOpenFile();
    });
  }

  // Initial load
  switchView('home');
  await raiseAppWindow();
});
