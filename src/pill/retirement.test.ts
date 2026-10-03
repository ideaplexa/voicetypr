import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
const base=resolve(import.meta.dirname,'../..');
const read=(file:string)=>readFileSync(resolve(base,file),'utf8');
describe('single floating feedback surface',()=>{
  it('has no retired entry points or components',()=>{for(const file of ['toast.html','src/toast.tsx','src/components/FeedbackToast.tsx','src/components/FeedbackToast.test.tsx'])expect(existsSync(resolve(base,file))).toBe(false);expect(read('vite.config.ts')).not.toContain('toast.html');});
  it('grants window capabilities only to current surfaces',()=>{for(const file of ['default','macos','windows'])expect(JSON.parse(read(`src-tauri/capabilities/${file}.json`)).windows).not.toContain('toast');});
  it('removes the native window, commands and raw floating transport',()=>{const files:string[]=[];const visit=(dir:string)=>{for(const entry of readdirSync(resolve(base,dir),{withFileTypes:true})){const path=`${dir}/${entry.name}`;if(entry.isDirectory())visit(path);else if(entry.name.endsWith('.rs'))files.push(path);}};visit('src-tauri/src');for(const file of files){const source=read(file);expect(source,file).not.toMatch(/\bpill_toast\b|\bPillToast\w*|get_webview_window\("toast"\)|hide_toast_window|emit\("toast"/);}expect(read('src/pill.tsx')).not.toMatch(/subscribe.*['"]toast['"]/);});
  it('leaves main-window sonner feedback installed',()=>{expect(read('src/services/updateService.ts')).toContain('from "sonner"');});
});
