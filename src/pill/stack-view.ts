import { FeedbackStack, type StackItem } from '@/pill/stack';
import { Spring, clamp } from '@/pill/spring';
import { icon } from '@/pill/icons';
import { OutlineClock } from '@/pill/outline-clock';
import type { CardCommand, FeedbackCard } from '@/pill/feedback';
import type { PillGeometry } from '@/pill-geometry';
type Rect = {x:number;y:number;width:number;height:number};
interface View { el: HTMLElement; w: Spring; h: Spring; y: Spring; scale: Spring; alpha: number; targetAlpha: number; key: string; clock?: OutlineClock }
export class StackView {
  readonly stack = new FeedbackStack();
  private views = new Map<string,View>();
  private container: HTMLElement;
  private more: HTMLElement;
  private timer: number | undefined;
  private geometry: PillGeometry = {anchor:'bottom-center',anchorX:220,anchorY:414};
  private alpha = 0;
  private handoff: string | undefined;
  private snapshot = '';
  constructor(root: HTMLElement, private deps: {wake():void; changed():void; call(c:CardCommand):Promise<unknown>; announce(s:string):void; setTimeout:typeof window.setTimeout; clearTimeout(id:number):void}) {
    this.container=document.createElement('div'); this.container.className='feedback-stack';
    this.more=document.createElement('span');this.more.className='stack-more';this.more.hidden=true;
    this.container.append(this.more);root.prepend(this.container);
  }
  get present() { return this.views.size > 0; }
  get active() { return this.stack.items.length > 0; }
  setGeometry(g:PillGeometry) { this.geometry=g; }
  push(c:FeedbackCard, origin?:Rect, announce = true) {
    const existing=this.stack.items.find(i=>i.key===c.key);
    this.stack.push(c,performance.now());
    if (!existing && announce) this.deps.announce([c.title,c.sub].filter(Boolean).join(' · '));
    this.sync();
    const v=this.views.get(c.key);
    if (origin && v) {
      v.w.x=origin.width;v.h.x=origin.height;v.y.x= this.geometry.anchor.startsWith('top-') ? -24 : 24;
      v.scale.x=1; v.alpha=1; this.alpha=1;this.handoff=c.key;
    }
    this.schedule();this.deps.changed();this.deps.wake();
  }
  remove(key:string) { this.stack.remove(key);this.sync();this.schedule();this.deps.changed();this.deps.wake(); }
  dropLive(generation?:number) { this.stack.dropLive(generation);this.sync();this.schedule();this.deps.changed();this.deps.wake(); }
  resolveMicStatus(stage:'new_take'|'recording',generation?:number) { this.stack.resolveMicStatus(stage,generation);this.sync();this.schedule();this.deps.changed();this.deps.wake(); }
  tuck(tucked:boolean) { if(tucked===this.stack.tucked)return; this.stack.tuck(tucked,performance.now());this.sync();this.schedule();this.deps.wake(); }
  private schedule() {
    if(this.timer!==undefined)this.deps.clearTimeout(this.timer);
    const delay=this.stack.nextDelay;
    this.timer=delay===null ? undefined : this.deps.setTimeout(()=>{this.timer=undefined;this.stack.advance(performance.now());this.sync();this.schedule();this.deps.changed();this.deps.wake();},Math.max(1,delay));
  }
  private fill(el:HTMLElement,item:StackItem): OutlineClock | undefined {
    el.className=`feedback-card ${item.tone}`;el.dataset.key=item.key;
    const glyph=document.createElement('span');glyph.className='card-glyph';glyph.innerHTML=icon(item.glyph ?? (item.key==='mic_dropped' || item.key==='mic_silent' ? 'mic-off' : item.key==='gpu_fallback' ? 'cpu' : item.key==='polish_skipped' ? 'star' : item.tone==='neutral' ? 'clock' : 'bang'),14);
    const txt=document.createElement('span');txt.className='card-text';
    const title=document.createElement('span');title.className='card-title';title.textContent=item.title;title.title=item.title;
    if(item.title.endsWith(' · Recording kept')) {title.textContent=item.title.slice(0,-17);const kept=document.createElement('span');kept.className='dim';kept.textContent=' · Recording kept';title.append(kept);}
    if(item.count>1 && !item.key.startsWith('escape_hint:')) {const count=document.createElement('span');count.className='card-count';count.textContent=`×${item.count}`;title.append(' ',count);}
    const sub=document.createElement('small');sub.textContent=item.sub;sub.hidden=!item.sub;txt.append(title);
    const actions=document.createElement('div');actions.className='card-actions';
    for(const a of item.actions) {
      const button=document.createElement('button');button.type='button';button.className='act';button.textContent=a.label;
      button.onclick=()=>{button.disabled=true;void this.deps.call(a.call).then(()=>this.remove(item.key)).catch(()=>{button.disabled=false;});};actions.append(button);
    }
    const dismiss=document.createElement('button');dismiss.type='button';dismiss.className='round card-dismiss';dismiss.setAttribute('aria-label','Dismiss');dismiss.innerHTML=icon('close',12);dismiss.onclick=()=>this.remove(item.key);
    if(!item.sub && !item.actions.length) {
      el.classList.add('single-row');el.replaceChildren(glyph,txt,dismiss);
    } else {
      actions.append(dismiss);const bottom=document.createElement('div');bottom.className='card-bottom';bottom.append(sub,actions);el.replaceChildren(glyph,txt,bottom);
    }
    if(item.key.startsWith('escape_hint:')) {
      el.classList.add('escape-hint-card');
      // The card's own outline is the 2 s countdown; a handed-off card starts partly drained.
      const clock=new OutlineClock(item.tone),remaining=Math.max(0,(item.expiresAt ?? performance.now()+2000)-performance.now());
      clock.start(2000,remaining);el.append(clock.el);return clock;
    }
    return undefined;
  }
  private sync() {
    const signature=JSON.stringify(this.stack.items.map(i=>[i.key,i.count,i.title,i.sub,i.expiresAt]));
    if(signature===this.snapshot)return;this.snapshot=signature;
    for(const item of this.stack.items) {
      let v=this.views.get(item.key);
      if(!v) {const el=document.createElement('div');this.container.append(el);v={el,w:new Spring(300),h:new Spring(54),y:new Spring(8),scale:new Spring(.96),alpha:0,targetAlpha:1,key:item.key};this.views.set(item.key,v);}
      const content=JSON.stringify([item.count,item.title,item.sub,item.expiresAt]);
      if(v.el.dataset.content!==content) {v.clock=this.fill(v.el,item);v.el.dataset.content=content;}
    }
    this.more.textContent=`+${this.stack.more}`;this.more.hidden=!this.stack.more;
  }
  step(dt:number,reduced:boolean,landed:boolean,enabled=true) {
    if(this.handoff && landed) this.handoff=undefined;
    const target=enabled && (this.stack.displayed.length || this.handoff) ? 1 : 0;
    this.alpha += Math.sign(target-this.alpha)*Math.min(Math.abs(target-this.alpha),dt/(target ? .18 : .12));
    let moving=this.alpha!==target;
    const wasPresent=this.present;
    const front=this.stack.displayed.slice(-1)[0];
    const frontView=front && this.views.get(front.key);
    // Measure once per content change, not on every animation frame.
    if(frontView && frontView.el.dataset.measured!==this.snapshot) {
      frontView.el.classList.remove('back');
      frontView.el.style.width='max-content';frontView.el.style.height='auto';
      const bounds=frontView.el.getBoundingClientRect();
      const width=Math.min(360,Math.max(236,bounds.width || 360));
      frontView.el.style.width=`${width}px`;
      frontView.el.dataset.naturalWidth=String(width);
      frontView.el.dataset.naturalHeight=String(frontView.el.getBoundingClientRect().height || (front.sub || front.actions.length ? 70 : 34));
      frontView.el.dataset.measured=this.snapshot;
    }
    const width=Number(frontView?.el.dataset.naturalWidth ?? 300),height=Number(frontView?.el.dataset.naturalHeight ?? 54);
    const top=this.geometry.anchor.startsWith('top-'), direction=top ? 1 : -1;
    for(const [key,v] of this.views) {
      const index=this.stack.displayed.findIndex(i=>i.key===key);
      const alive=this.stack.items.some(i=>i.key===key), displayed=index>=0, depth=displayed ? this.stack.displayed.length-1-index : 0;
      v.targetAlpha=(displayed && depth<3 || alive && key===this.handoff) ? 1 : 0;
      v.w.to=width;v.h.to=height;v.y.to=direction*Math.min(2,depth)*19;v.scale.to=1-Math.min(2,depth)*.05;
      for(const s of [v.w,v.h,v.y,v.scale]) moving=s.step(dt,reduced)||moving;
      v.alpha+=Math.sign(v.targetAlpha-v.alpha)*Math.min(Math.abs(v.targetAlpha-v.alpha),dt/(alive ? .2 : .08));
      moving=v.alpha!==v.targetAlpha||moving;
      v.el.hidden=depth>2 || v.alpha===0;v.el.inert=!displayed || depth!==0;
      v.el.classList.toggle('back',depth>0);v.el.style.zIndex=String(alive ? 3-depth : 4);
      v.el.style.width=`${Math.max(12,v.w.x)}px`;v.el.style.height=`${Math.max(12,v.h.x)}px`;
      v.clock?.size(Math.max(12,v.w.x),Math.max(12,v.h.x),20);
      v.el.style.opacity=String(v.alpha);v.el.style.transform=`translateY(${v.y.x}px) scale(${v.scale.x})`;
      v.el.style.transformOrigin=top ? '50% 0' : '50% 100%';
      if(!alive && v.alpha===0){v.el.remove();this.views.delete(key);}
    }
    this.container.style.left=`${this.geometry.anchorX}px`;this.container.style.top=`${this.geometry.anchorY+direction*24}px`;
    this.container.dataset.anchor=this.geometry.anchor;
    this.container.style.opacity=String(clamp(this.alpha));this.container.inert=target===0;
    this.more.textContent=`+${Math.max(0,this.stack.displayed.length-3)}`;this.more.hidden=this.stack.displayed.length<=3;
    this.more.style[top?'top':'bottom']=`${height+44}px`;
    if(wasPresent && !this.present) this.deps.changed();
    return moving;
  }
  /** Card rectangles from the spring values and the CSS anchor rules; never measured from the DOM. */
  rects():Rect[] {
    if(this.container.inert)return [];
    const top=this.geometry.anchor.startsWith('top-'),centre=this.geometry.anchor.endsWith('-center'),right=this.geometry.anchor.endsWith('-right');
    const cx=this.geometry.anchorX,cy=this.geometry.anchorY+(top ? 24 : -24);
    const out:Rect[]=[];
    for(const v of this.views.values()) {
      if(v.el.hidden)continue;
      // Boxes hug the container's corner; scale pivots on the anchored edge, translateY is unscaled.
      const w=Math.max(12,v.w.x),h=Math.max(12,v.h.x),s=v.scale.x,sw=w*s,sh=h*s;
      const left=centre ? cx-w/2 : right ? cx-w : cx,boxTop=top ? cy : cy-h;
      out.push({x:left+w/2-sw/2,y:(top ? boxTop : boxTop+h-sh)+v.y.x,width:sw,height:sh});
    }
    return out;
  }
  pointer(x?:number,y?:number) { if(this.container.inert&&!this.views.size)return; for(const b of this.container.querySelectorAll('button')) {const r=b.getBoundingClientRect();b.classList.toggle('hov',x!==undefined&&y!==undefined&&x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom);} }
  destroy(){if(this.timer!==undefined)this.deps.clearTimeout(this.timer);this.container.remove();}
}
