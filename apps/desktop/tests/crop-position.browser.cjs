const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require('playwright-core');
const root = path.resolve(__dirname, '../../..');
const ts = require('typescript');
const code = ts.transpileModule(fs.readFileSync(path.join(root, 'apps/desktop/src/editor.ts'), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const css = fs.readFileSync(path.join(root, 'apps/desktop/src/style.css'), 'utf8');
(async () => {
  const browser = await chromium.launch({ channel: process.env.MARKITS_TEST_BROWSER || 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1300, height: 900 } });
    for (const zoom of [0.7, 1, 1.4]) {
      await page.setContent(`<style>${css}
      #viewport {position:absolute;left:195px;top:80px;width:835px;height:790px}</style>
      <div id="viewport" class="canvas-viewport"><div id="canvas" class="canvas-container">
        <img id="image" class="bg-image"><div id="overlay" class="editor-crop-overlay" style="display:none"><div id="box" class="crop-box"></div></div>
      </div></div>`);
      await page.evaluate(code => {
        const exports = {};
        new Function('exports', 'require', code)(exports, () => ({t: value => value}));
        window.Editor = exports.AnnotationEditor;
      }, code);
      const result = await page.evaluate(async zoom => {
        const editor = Object.create(window.Editor.prototype);
        const canvas = document.getElementById('canvas');
        const image = document.getElementById('image');
        const makeImage = (w, h) => 'data:image/svg+xml,' + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="white"/></svg>`);
        image.src = makeImage(780, 103); await image.decode();
        Object.assign(canvas.style, {display:'block',width:'780px',height:'103px'});
        Object.assign(editor, {
          scene: {canvas:{width:780,height:103},annotations:[]}, outputWidth:780, outputHeight:103,zoomLevel:1,
          baseImageState:{dataUrl:'base',width:1720,height:1389,uiElements:[]},
          cropHistoryStack:[{offset:{x:284,y:590}}], isAutoCropped:false,
          autoCropOffset:{x:0,y:0}, canvasContainerEl:canvas, bgImgEl:image,
          editorCropOverlayEl:document.getElementById('overlay'),cropBoxEl:document.getElementById('box'),
          viewportEl:document.getElementById('viewport'),
          hasImage:()=>true,setHoverPreview(){},updateHandles(){},updateInspector(){},renderAllSnapTargets(){},
          revertCrop(){
            this.scene.canvas={width:1720,height:1389};this.outputWidth=1720;this.outputHeight=1389;
            canvas.style.width='1720px';canvas.style.height='1389px';image.src=makeImage(1720,1389);
            this.setZoom(this.zoomLevel);
          },
        });
        editor.setZoom(zoom);
        const before=canvas.getBoundingClientRect().toJSON();
        editor.startCropMode();
        await image.decode();
        await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);
        const after=document.getElementById('box').getBoundingClientRect().toJSON();
        return {before,after,delta:{x:after.left-before.left,y:after.top-before.top,width:after.width-before.width,height:after.height-before.height},canvasWidth:canvas.clientWidth};
      }, zoom);
      console.log(JSON.stringify({zoom,...result}));
      for (const delta of Object.values(result.delta)) assert.ok(Math.abs(delta)<1, String(delta));
    }
  } finally { await browser.close(); }
})().catch(error => {console.error(error);process.exitCode=1;});


