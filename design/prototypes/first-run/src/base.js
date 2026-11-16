(function () {
  "use strict";
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return Array.from((r || document).querySelectorAll(s)); };
  var reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  var esc = function (s) { return String(s).replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); };
  var VT = (window.VT = { $: $, $$: $$, reduce: reduce, esc: esc, gen: 0, modal: false, talk: null, failTrial: false });

  var MARK = '<svg viewBox="100 96 312 320" fill="none" aria-hidden="true"><g stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"><path d="M143 197 C166 288 201 350 240 393" stroke-width="36"/><path d="M369 197 C346 288 311 350 272 393" stroke-width="36"/><path d="M256 163 L256 338" stroke-width="23"/><path d="M202 226 L202 257" stroke-width="15"/><path d="M226 207 L226 276" stroke-width="15"/><path d="M286 207 L286 276" stroke-width="15"/><path d="M310 226 L310 257" stroke-width="15"/></g><circle cx="256" cy="124" r="18" fill="currentColor"/></svg>';
  var X = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/></svg>';
  var CHECK = '<svg class="tick" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7"/></svg>';
  var ARC = '<svg class="arc" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M8 2.5a5.5 5.5 0 1 1-5.5 5.5"/></svg>';
  VT.icons = { mark: MARK, check: CHECK, arc: ARC };

  /* ---------- state ---------- */
  var HOTKEYS = {
    mac: [["\u2325", "Space"], ["Right \u2318"], ["Fn"]],
    windows: [["Ctrl", "Space"], ["Right Alt"], ["F9"]],
  };
  VT.USES = [
    { id: "messages", label: "Messages and email", does: "Starts you on the Message style: tidy sentences in your own wording." },
    { id: "ai", label: "Prompts for AI", does: "Starts you on Clean, with clean-up-and-clarify for AI apps (planned for 2.1)." },
    { id: "code", label: "Code and tech notes", does: "Starts you on the Code style, with developer words like useEffect and kubectl." },
    { id: "docs", label: "Notes and documents", does: "Starts you on the Writing style for longer text." },
    { id: "clinical", label: "Medical or legal work", does: "Starts you on Clean, with medical and legal words." },
    { id: "mix", label: "A bit of everything", does: "Starts you on Clean, the default." },
  ];
  VT.trialEnds = "Monday 23 November";
  function fresh(platform) {
    return { platform: platform, hotkey: 0, hold: true, mic: false, micDenied: false, ax: false, axSkipped: false, use: null, trial: false, name: "", email: "" };
  }
  VT.state = fresh(/Win/i.test(navigator.platform || "") ? "windows" : "mac");
  VT.caps = function () { return HOTKEYS[VT.state.platform][VT.state.hotkey]; };
  VT.capsHTML = function () {
    var caps = VT.caps();
    return '<span class="caps" aria-label="' + esc(caps.join(" + ")) + '">' + caps.map(function (c) { return '<kbd class="cap live">' + esc(c) + "</kbd>"; }).join("") + "</span>";
  };
  VT.paint = function () {
    var s = VT.state;
    $$("[data-caps]").forEach(function (e) { e.innerHTML = VT.capsHTML(); });
    $$("[data-verb]").forEach(function (e) { e.textContent = s.hold ? (e.dataset.verb === "lower" ? "hold" : "Hold") : (e.dataset.verb === "lower" ? "press" : "Press"); });
    $$("[data-after]").forEach(function (e) { e.textContent = s.hold ? "let go" : "press again"; });
    $$("[data-os]").forEach(function (e) { e.textContent = s.platform === "mac" ? "Mac" : "PC"; });
    $$("[data-mac-only]").forEach(function (e) { e.hidden = s.platform !== "mac"; });
    $$("[data-result]").forEach(function (e) { e.textContent = VT.copyOnly() ? "copy" : "paste"; });
    $$("[data-degraded]").forEach(function (e) { e.hidden = !VT.copyOnly(); });
    $$('[role="radiogroup"]').forEach(function (g) {
      var rs = $$('[role="radio"]', g), on = rs.filter(function (r) { return r.getAttribute("aria-checked") === "true"; })[0] || rs[0];
      rs.forEach(function (r) { r.tabIndex = r === on ? 0 : -1; });
    });
  };
  VT.copyOnly = function () { return VT.state.platform === "mac" && !VT.state.ax && VT.state.axSkipped; };
  VT.later = function (fn, ms) {
    var g = VT.gen;
    return setTimeout(function () { if (g === VT.gen) fn(); }, ms);
  };
  VT.announce = function (text) { var a = $("#vt-announce"); if (a) { a.textContent = ""; setTimeout(function () { a.textContent = text; }, 50); } };
  VT.wait = function (ms) { return new Promise(function (r) { VT.later(r, ms); }); };

  /* ---------- pill ---------- */
  var wave = null, pillSeq = 0;
  function pillSet(state, html, cls) {
    var el = $("#pill");
    if (!el) return;
    if (wave) { wave.stop(); wave = null; }
    pillSeq++;
    var w0 = el.offsetWidth, h0 = el.offsetHeight;
    el.dataset.state = state;
    el.className = "pill" + (cls ? " " + cls : "");
    var ico = /^(listening|work|done)$/.test(state) ? '<span class="pico" aria-hidden="true">' + MARK + "<i>✦</i></span>" : "";
    el.innerHTML = '<div class="pill-body">' + ico + html + "</div>";
    el.style.transition = "none"; el.style.width = "auto"; el.style.height = "auto";
    var w1 = el.offsetWidth, h1 = el.offsetHeight;
    el.style.width = w0 + "px"; el.style.height = h0 + "px";
    void el.offsetWidth;
    el.style.transition = "";
    el.style.width = w1 + "px"; el.style.height = h1 + "px";
    var live = $("#pill-live");
    if (live) live.textContent = el.textContent.trim();
  }
  function startWave() {
    var el = $("#pill"), c = $("canvas", el), timeEl = $(".time", el);
    var dpr = window.devicePixelRatio || 1;
    c.width = 96 * dpr; c.height = 24 * dpr;
    var g = c.getContext("2d");
    g.scale(dpr, dpr);
    var bars = [], level = 0, target = 0, last = performance.now(), acc = 0, t0 = last, raf = 0;
    function frame(now) {
      var dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      level += (target - level) * (1 - Math.exp(-dt / (target > level ? 0.03 : 0.18)));
      target *= Math.pow(0.03, dt);
      acc += dt;
      while (acc > 0.07) { acc -= 0.07; bars.unshift(level > 0.05 ? 3 + 15 * level * (0.55 + 0.45 * Math.random()) : 2); if (bars.length > 26) bars.pop(); }
      g.clearRect(0, 0, 96, 24);
      g.fillStyle = "#fff";
      if (reduce) {
        for (var k = 0; k < 11; k++) {
          var lv = Math.min(1, level * 1.6) * (0.5 + 0.5 * Math.sin(Math.PI * (k + 0.5) / 11)), hh = 2 + 14 * lv;
          g.globalAlpha = lv > 0.03 ? 0.93 : 0.34; g.beginPath(); g.roundRect(8 + k * 8, 12 - hh / 2, 2.4, hh, 1.2); g.fill();
        }
        timeEl.textContent = "0:" + String(Math.floor((now - t0) / 1000)).padStart(2, "0");
        raf = requestAnimationFrame(frame);
        return;
      }
      var shift = (acc / 0.07) * 4;
      for (var i = 0; i < bars.length; i++) {
        var x = 96 - 4 - i * 4 - shift;
        if (x < 0) break;
        g.globalAlpha = (bars[i] > 2 ? 0.93 : 0.34) * Math.min(1, x / 18);
        g.beginPath(); g.roundRect(x, 12 - bars[i] / 2, 2.4, bars[i], 1.2); g.fill();
      }
      timeEl.textContent = "0:" + String(Math.floor((now - t0) / 1000)).padStart(2, "0");
      raf = requestAnimationFrame(frame);
    }
    raf = requestAnimationFrame(frame);
    wave = { bump: function () { target = 0.55 + 0.45 * Math.random(); }, stop: function () { cancelAnimationFrame(raf); } };
  }
  VT.pill = {
    idle: function () { pillSet("idle", '<i class="core"></i>'); },
    hint: function () {
      var keys = VT.caps().map(function (c) { return '<span class="kc">' + esc(c) + "</span>"; }).join("");
      pillSet("hint", '<span class="dim">' + (VT.state.hold ? "Hold" : "Press") + "</span>" + keys + '<span class="dim">to talk</span>');
    },
    listening: function () {
      pillSet("listening", '<canvas aria-hidden="true"></canvas><span class="time">0:00</span><button type="button" class="pbtn stop" data-pill="stop" aria-label="Stop and type it">Stop</button><button type="button" class="pbtn cancel" data-pill="cancel" aria-label="Cancel and discard">' + X + "</button>");
      startWave();
    },
    escHint: function () {
      var h = $("#pill-hint");
      if (!h) return;
      h.textContent = "Press Esc again to discard";
      h.hidden = false;
      clearTimeout(h.t);
      h.t = setTimeout(function () { h.hidden = true; }, 2000);
    },
    bump: function () { if (wave) wave.bump(); },
    work: function (label, polish) { pillSet("work", ARC.replace('class="arc"', 'class="arc' + (polish ? " slow" : "") + '"') + "<span>" + esc(label) + "</span>"); },
    done: function (words, then) {
      pillSet("done", CHECK + "<span>Pasted \u00b7 " + words + " words</span>");
      var seq = pillSeq;
      VT.later(function () { if (seq === pillSeq) (then || VT.pill.idle)(); }, 2400);
    },
    note: function (text, then) {
      pillSet("note", '<svg class="warn" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><circle cx="8" cy="8" r="5.5"/><path d="M8 5.2v3.2M8 10.8v.1"/></svg><span>' + esc(text) + "</span>", "amber");
      var seq = pillSeq;
      VT.later(function () { if (seq === pillSeq) (then || VT.pill.idle)(); }, 2200);
    },
  };

  /* ---------- what the OS shows ---------- */
  function overlay(html) {
    var o = document.createElement("div");
    o.className = "os";
    o.innerHTML = html;
    document.body.appendChild(o);
    VT.modal = true;
    var app = $("#app");
    if (app) app.inert = true;
    var prev = document.activeElement;
    o.addEventListener("keydown", function (e) {
      if (e.key !== "Tab") return;
      var f = $$("button", o);
      if (!f.length) return;
      var i = f.indexOf(document.activeElement);
      e.preventDefault();
      f[(i + (e.shiftKey ? -1 : 1) + f.length) % f.length].focus();
    });
    var api = {
      el: o,
      close: function () {
        if (api.closed) return;
        api.closed = true;
        o.classList.add("closing");
        setTimeout(function () { o.remove(); }, 150);
        VT.modal = false;
        if (app) app.inert = false;
        document.removeEventListener("keydown", api.onKey, true);
        if (prev && prev.isConnected && prev.focus) prev.focus({ preventScroll: true });
      },
    };
    return api;
  }
  VT.askMic = function () {
    return new Promise(function (res) {
      if (VT.state.mic) return res(true);
      if (VT.state.platform === "windows") { VT.state.mic = true; return res(true); }
      var o = overlay('<div class="os-note">macOS asks this once, the first time you talk</div><div class="os-alert" role="alertdialog" aria-modal="true" aria-label="Microphone access"><div class="os-icon">' + MARK + "</div><b>\u201cVoicetypr\u201d would like to access the microphone.</b><p>Voicetypr listens only while you hold your shortcut.</p><div class=\"os-btns\"><button type=\"button\" data-a=\"no\">Don\u2019t Allow</button><button type=\"button\" class=\"go\" data-a=\"yes\">Allow</button></div></div>");
      var finish = function (ok) { VT.state.mic = ok; VT.state.micDenied = !ok; o.close(); res(ok); };
      $('[data-a="yes"]', o.el).addEventListener("click", function () { finish(true); });
      $('[data-a="no"]', o.el).addEventListener("click", function () { finish(false); });
      o.onKey = function (e) { if (e.key === "Escape") { e.stopPropagation(); finish(false); } };
      document.addEventListener("keydown", o.onKey, true);
      $('[data-a="yes"]', o.el).focus();
    });
  };
  VT.openSettings = function (kind) {
    var isMic = kind === "mic";
    return new Promise(function (res) {
      var o = overlay('<div class="os-note">System Settings opens on the right page</div><div class="os-sheet" role="dialog" aria-modal="true" aria-label="System Settings"><header><span>Privacy &amp; Security \u203a</span> ' + (isMic ? "Microphone" : "Accessibility") + '</header><div class="body"><p>' + (isMic ? "Allow the applications below to access your microphone." : "Allow the applications below to control your computer.") + '</p><div class="os-row"><div class="os-icon">' + MARK + '</div><span>Voicetypr</span><button type="button" class="os-switch" role="switch" aria-checked="false" aria-label="Voicetypr"></button></div></div><footer><button type="button" data-a="done">Done</button></footer></div>');
      var sw = $(".os-switch", o.el), on = false;
      var finish = function () { if (isMic) { VT.state.mic = on; VT.state.micDenied = !on; } else VT.state.ax = on; o.close(); res(on); };
      sw.addEventListener("click", function () {
        on = !on;
        sw.setAttribute("aria-checked", String(on));
        if (on) setTimeout(finish, 620);
      });
      $('[data-a="done"]', o.el).addEventListener("click", finish);
      o.onKey = function (e) { if (e.key === "Escape") { e.stopPropagation(); finish(); } };
      document.addEventListener("keydown", o.onKey, true);
      sw.focus();
    });
  };

  /* ---------- shortcut popover ---------- */
  VT.editHotkey = function (anchor, onChange) {
    var old = $(".pop");
    if (old) { old.remove(); return; }
    var p = document.createElement("div");
    p.className = "pop";
    p.setAttribute("role", "dialog");
    p.setAttribute("aria-label", "Shortcut");
    var draw = function () {
      var s = VT.state;
      p.innerHTML = "<h3>Your shortcut</h3><div class=\"opts\" role=\"radiogroup\">" + HOTKEYS[s.platform].map(function (caps, i) {
        return '<button type="button" class="opt" role="radio" aria-checked="' + (i === s.hotkey) + '" data-i="' + i + '"><span>' + (i === 0 ? "Recommended" : i === 1 ? "One key, right hand" : "One key, top row") + '</span><span class="caps">' + caps.map(function (c) { return '<kbd class="cap">' + esc(c) + "</kbd>"; }).join("") + "</span></button>";
      }).join("") + '</div><div class="seg"><button type="button" aria-pressed="' + s.hold + '" data-hold-mode="1">Hold to talk</button><button type="button" aria-pressed="' + !s.hold + '" data-hold-mode="0">Press to start and stop</button></div><div class="foot"><span>Esc twice cancels a recording</span><button type="button" class="btn sm" data-a="done">Done</button></div>';
    };
    draw();
    document.body.appendChild(p);
    var r = anchor.getBoundingClientRect();
    p.style.left = Math.max(12, Math.min(innerWidth - 312, r.left + r.width / 2 - 150)) + "px";
    var below = r.bottom + 8 + p.offsetHeight < innerHeight - 12;
    p.style.top = (below ? r.bottom + 8 : Math.max(12, r.top - 8 - p.offsetHeight)) + "px";
    if (!below) p.style.transformOrigin = "50% 100%";
    var close = function () { p.remove(); document.removeEventListener("pointerdown", away, true); document.removeEventListener("keydown", key, true); if (anchor.isConnected) anchor.focus({ preventScroll: true }); };
    var away = function (e) { if (!p.contains(e.target) && e.target !== anchor) close(); };
    var key = function (e) { if (e.key === "Escape") { e.stopPropagation(); close(); } };
    p.addEventListener("click", function (e) {
      var b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.a === "done") return close();
      if (b.dataset.i) VT.state.hotkey = +b.dataset.i;
      if (b.dataset.holdMode) VT.state.hold = b.dataset.holdMode === "1";
      draw(); VT.paint();
      if (onChange) onChange();
      var f = $('[aria-checked="true"]', p);
      if (f) f.focus();
    });
    document.addEventListener("pointerdown", away, true);
    document.addEventListener("keydown", key, true);
    $('[aria-checked="true"]', p).focus();
  };

  VT.toast = function (msg) {
    var old = $(".toast");
    if (old) old.remove();
    var t = document.createElement("div");
    t.className = "toast"; t.setAttribute("role", "status"); t.textContent = msg;
    document.body.appendChild(t);
    setTimeout(function () { t.remove(); }, 3200);
  };

  /* ---------- engine getting ready in the background ---------- */
  var enginePct = 14, engineTimer = 0;
  VT.engineReady = function () { return enginePct >= 100; };
  VT.enginePct = function () { return Math.round(enginePct); };
  function paintEngine() {
    $$("[data-engine]").forEach(function (e) {
      var ready = enginePct >= 100;
      e.classList.add("engine");
      e.classList.toggle("ready", ready);
      e.innerHTML = '<span class="bar"><i style="width:' + enginePct + '%"></i></span><span class="dot"></span><span>' + (ready ? "Offline dictation ready on this " + (VT.state.platform === "mac" ? "Mac" : "PC") : "Preparing offline dictation \u00b7 " + Math.round(enginePct) + "%") + "</span>";
    });
  }
  function runEngine() {
    clearInterval(engineTimer);
    enginePct = 14;
    paintEngine();
    engineTimer = setInterval(function () {
      enginePct = Math.min(100, enginePct + 3 + Math.random() * 6);
      paintEngine();
      if (enginePct >= 100) clearInterval(engineTimer);
    }, 800);
  }
  VT.paintEngine = paintEngine;

  VT.fieldsHTML = function () {
    return '<div class="fields" data-trial-fields><div class="fld"><label for="f-name">Name</label><input id="f-name" data-autofocus autocomplete="name" placeholder="Priya Raman" aria-describedby="f-name-msg" required><span class="msg" id="f-name-msg" role="alert"></span></div><div class="fld"><label for="f-email">Email</label><input id="f-email" type="email" autocomplete="email" placeholder="priya@example.com" aria-describedby="f-email-msg" required><span class="msg" id="f-email-msg" role="alert"></span></div></div>';
  };
  VT.checkFields = function () {
    var tb = $("#trial-btn");
    if (tb && tb.getAttribute("aria-busy") === "true") return false;
    var n = $("#f-name"), m = $("#f-email"), bad = [];
    var name = n.value.trim(), email = m.value.trim();
    var set = function (el, msg) { el.setAttribute("aria-invalid", msg ? "true" : "false"); $("#" + el.id + "-msg").textContent = msg; if (msg) bad.push(el); };
    set(n, name ? "" : "Enter your name");
    set(m, !email ? "Enter your email" : m.checkValidity() && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? "" : "That email doesn’t look right");
    if (bad.length) { bad[0].focus(); return false; }
    VT.state.name = name; VT.state.email = email;
    return true;
  };
  VT.firstName = function () { return VT.state.name.split(/\s+/)[0]; };
  document.addEventListener("input", function (e) { if (e.target.closest && e.target.closest("[data-trial-fields]") && e.target.getAttribute("aria-invalid") === "true") { e.target.setAttribute("aria-invalid", "false"); $("#" + e.target.id + "-msg").textContent = ""; } });
  document.addEventListener("keydown", function (e) { if (e.key === "Enter" && e.target.closest && e.target.closest("[data-trial-fields]")) { e.preventDefault(); var b = $("#trial-btn"); if (b) b.click(); } });
  VT.startTrial = function () {
    return new Promise(function (res, rej) {
      VT.later(function () { if (VT.failTrial) rej(new Error("offline")); else { VT.state.trial = true; res(); } }, 1100);
    });
  };

  /* ---------- scenes ---------- */
  VT.go = function (id) {
    var cur = $(".scene.active"), next = $('.scene[data-id="' + id + '"]');
    if (!next || cur === next) return;
    var show = function () {
      $$(".scene").forEach(function (s) { s.hidden = s !== next; s.classList.remove("active", "leaving"); });
      $$("[data-rise]", next).forEach(function (e, i) { e.style.setProperty("--i", e.dataset.rise || i); });
      next.classList.add("active");
      VT.scene = id;
      document.body.dataset.scene = id;
      if (VT.onScene) VT.onScene(id);
      var f = $("[data-autofocus]", next) || $("h1,h2", next);
      if (f) { if (/^H/.test(f.tagName)) f.tabIndex = -1; f.focus({ preventScroll: true }); }
    };
    if (cur && !reduce) { cur.classList.add("leaving"); VT.later(show, 200); } else show();
  };

  /* ---------- FLIP ---------- */
  VT.flip = function (els, mutate, opt) {
    opt = opt || {};
    var first = els.map(function (e) { return e.getBoundingClientRect(); });
    mutate();
    if (reduce) return Promise.resolve();
    var dur = opt.dur || 620, stagger = opt.stagger || 0;
    els.forEach(function (e, i) {
      var last = e.getBoundingClientRect(), dx = first[i].left - last.left, dy = first[i].top - last.top;
      if (!dx && !dy) return;
      e.animate([{ transform: "translate(" + dx + "px," + dy + "px)" }, { transform: "none" }], { duration: dur, delay: i * stagger, easing: "cubic-bezier(.16,1,.3,1)", fill: "backwards" });
    });
    return VT.wait(dur + els.length * stagger);
  };

  /* ---------- a sentence that cleans itself up in place ---------- */
  VT.Sentence = function (el, tokens) {
    var self = this, words = [];
    el.innerHTML = "";
    el.classList.remove("polished");
    this.words = words;
    this.add = function (i) {
      var t = tokens[i], w = document.createElement("span");
      w.className = "w new" + (t.p ? "" : " drop");
      w.innerHTML = '<span class="t">' + esc(t.r) + "</span>";
      w.tok = t;
      el.appendChild(w);
      el.appendChild(document.createTextNode(" "));
      words.push(w);
    };
    this.polished = false;
    this.polish = function () {
      var drops = words.filter(function (w) { return !w.tok.p; }), kept = words.filter(function (w) { return w.tok.p; });
      self.polished = true;
      drops.forEach(function (w, k) { VT.later(function () { w.classList.add("cut"); }, k * 55); });
      return VT.wait(drops.length * 55 + 420).then(function () {
        drops.forEach(function (w) { w.classList.add("gone"); });
        return VT.wait(200);
      }).then(function () {
        return VT.flip(kept, function () {
          el.classList.add("polished");
          drops.forEach(function (w) { w.classList.add("out"); });
          kept.forEach(function (w) { if (w.tok.p !== w.tok.r) { $(".t", w).textContent = w.tok.p; w.classList.add("changed"); } });
        }, { stagger: 9 });
      });
    };
    this.raw = function () {
      var drops = words.filter(function (w) { return !w.tok.p; }), kept = words.filter(function (w) { return w.tok.p; });
      self.polished = false;
      return VT.flip(kept, function () {
        el.classList.remove("polished");
        drops.forEach(function (w) { w.classList.remove("out"); });
        kept.forEach(function (w) { $(".t", w).textContent = w.tok.r; w.classList.remove("changed"); });
      }, { stagger: 6 }).then(function () {
        drops.forEach(function (w) { w.classList.remove("gone"); });
        return VT.wait(260);
      });
    };
  };
  VT.rawHTML = function (tokens) {
    return tokens.map(function (t) { return t.p ? esc(t.r) : "<s>" + esc(t.r) + "</s>"; }).join(" ");
  };
  VT.polishedText = function (tokens) {
    return tokens.filter(function (t) { return t.p; }).map(function (t) { return t.p; }).join(" ");
  };

  /* ---------- one dictation ---------- */
  VT.Dictation = function (o) {
    var st = "idle", i = 0, auto = false, self = this, run = 0, releasedAt = 0, escAt = 0;
    var count = o.tokens.filter(function (t) { return t.p; }).length;
    var at = function (fn, ms) { var r = run; VT.later(function () { if (r === run) fn(); }, ms); };
    function say() {
      if (st !== "listening") return;
      if (i >= o.tokens.length) { if (auto) release(); return; }
      o.onWord(i, o.tokens[i]);
      VT.pill.bump();
      var len = o.tokens[i].r.length;
      i++;
      at(say, 150 + len * 26 + Math.random() * 90);
    }
    function begin(isAuto) {
      run++; auto = isAuto; st = "listening"; i = 0;
      if (o.onStart) o.onStart();
      VT.pill.listening();
      at(say, 340);
    }
    function release(stopped) {
      if (st !== "listening") return;
      if (i < 3 && (!auto || stopped)) {
        run++; st = "idle";
        VT.pill.note("Too short — " + (VT.state.hold ? "hold" : "talk") + " a bit longer", VT.pill.hint);
        if (o.onShort) o.onShort();
        return;
      }
      st = "flushing";
      releasedAt = performance.now();
      if (stopped) { finish(); return; }
      flush();
    }
    function flush() {
      if (i < o.tokens.length) { o.onWord(i, o.tokens[i]); i++; at(flush, 42); return; }
      finish();
    }
    function finish() {
      st = "working";
      if (!VT.engineReady()) {
        VT.pill.work("Getting ready · " + VT.enginePct() + "%");
        if (o.onPhase) o.onPhase("preparing");
        at(finish, 300);
        return;
      }
      VT.pill.work("Transcribing");
      if (o.onPhase) o.onPhase("transcribing");
      at(function () {
        VT.pill.work("Polishing · Clean", true);
        if (o.onPhase) o.onPhase("polishing");
        at(function () {
          st = "done";
          VT.pill.done(count);
          o.onDone(count, ((performance.now() - releasedAt) / 1000).toFixed(1));
        }, 950);
      }, 560);
    }
    this.state = function () { return st; };
    this.reset = function () { run++; st = "idle"; i = 0; };
    this.cancel = function () {
      if (st !== "listening" && st !== "flushing" && st !== "working") return;
      run++; st = "idle"; i = 0;
      VT.pill.hint();
      VT.announce("Cancelled. Nothing was typed");
      if (o.onCancel) o.onCancel();
    };
    this.stop = function () { if (st === "listening") release(true); };
    this.esc = function () {
      if (st !== "listening") return false;
      var now = performance.now();
      if (now - escAt < 2000) { escAt = 0; self.cancel(); } else { escAt = now; VT.pill.escHint(); }
      return true;
    };
    this.down = function (src) {
      if (st !== "idle") return;
      if (VT.state.mic) return begin(src === "auto");
      VT.askMic().then(function (ok) {
        if (o.onMic) o.onMic(ok);
        if (ok && src !== "key") begin(true);
      });
    };
    this.up = function (quick) {
      if (st !== "listening") return;
      if (quick) { auto = true; if (i >= o.tokens.length) release(); } else release();
    };
    this.arm = function () { VT.talk = self; };
  };

  /* ---------- holding the shortcut ---------- */
  var holding = false, holdSrc = "", downAt = 0;
  function talkKey(e) {
    if (e.code === "Space") return true;
    var s = VT.state;
    if (s.hotkey === 1) return s.platform === "mac" ? e.code === "MetaRight" : e.code === "AltRight";
    if (s.hotkey === 2) return s.platform === "windows" && e.code === "F9";
    return false;
  }
  function down(src) {
    if (holding || !VT.talk || VT.modal) return;
    holding = true; holdSrc = src; downAt = performance.now();
    document.body.classList.add("holding");
    VT.talk.down(src);
  }
  function up() {
    if (!holding) return;
    holding = false;
    document.body.classList.remove("holding");
    if (VT.talk) VT.talk.up(holdSrc === "pointer" && performance.now() - downAt < 240);
  }
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && VT.talk && !VT.modal && !$(".pop") && VT.talk.esc()) { e.preventDefault(); return; }
    var t = VT.talk;
    if (t && talkKey(e) && !VT.modal && !$(".pop") && !/^(INPUT|TEXTAREA)$/.test(e.target.tagName)) {
      var live = t.state() === "listening";
      if (e.target.tagName === "BUTTON" && !e.target.hasAttribute("data-hold") && !live) return;
      if (e.target.tagName === "BUTTON" && t.state() !== "idle" && !live) return;
      e.preventDefault();
      if (e.repeat) return;
      if (!VT.state.hold) {
        if (live) t.stop();
        else if (t.state() === "idle") t.down(VT.state.mic ? "auto-toggle" : "key");
        return;
      }
      down("key");
      return;
    }
    if (e.key === "Enter" && e.target.closest && e.target.closest("[data-hold]") && t) { e.preventDefault(); t.down("auto"); return; }
    if (e.key === "Enter" && e.target === document.body && !VT.modal) { var pr = $(".scene.active [data-primary]"); if (pr) pr.click(); }
    var radio = e.target.closest && e.target.closest('[role="radio"]');
    if (radio && /^Arrow/.test(e.key)) {
      var rs = $('[role="radio"]', radio.closest('[role="radiogroup"]')), k = rs.indexOf(radio);
      var n = rs[(k + (e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 1) + rs.length) % rs.length];
      e.preventDefault(); n.focus(); n.click();
    }
  });
  document.addEventListener("keyup", function (e) { if (talkKey(e) && VT.state.hold) { if (holding) e.preventDefault(); up(); } });
  document.addEventListener("pointerdown", function (e) {
    var h = e.target.closest && e.target.closest("[data-hold]");
    if (!h || e.button !== 0 || !VT.talk) return;
    e.preventDefault(); h.focus({ preventScroll: true });
    if (!VT.state.hold) { if (VT.talk.state() === "listening") VT.talk.stop(); else VT.talk.down("auto"); return; }
    down("pointer");
  });
  document.addEventListener("click", function (e) {
    var p = e.target.closest && e.target.closest("[data-pill]");
    if (p && VT.talk) { if (p.dataset.pill === "stop") VT.talk.stop(); else VT.talk.cancel(); return; }
    var d = e.target.closest && e.target.closest("[data-doc]");
    if (d) { VT.toast((d.dataset.doc === "terms" ? "Terms" : "Privacy Policy") + " opens on voicetypr.com in your browser"); return; }
    if (e.target.closest && e.target.closest("[data-shared]")) VT.shared(e.target.closest("[data-shared]"));
  });
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
  window.addEventListener("blur", up);

  /* ---------- What's shared, the app's own copy (src/components/WhatsShared.tsx) ---------- */
  VT.shared = function (anchor) {
    var o = document.createElement("div");
    o.className = "os app-dialog";
    o.innerHTML = '<div class="dlg" role="dialog" aria-modal="true" aria-labelledby="dlg-t"><h3 id="dlg-t">What’s shared</h3><p>Anonymous information goes to PostHog in the EU to help us find and fix problems.</p><ul><li>App version, operating system and device category.</li><li>Feature choices, dictation outcomes, word count ranges and speed numbers.</li><li>Fixed error codes and crash locations inside Voicetypr.</li><li>Random installation and dictation IDs that connect these events.</li></ul><p>Never your words, audio, clipboard, prompts, keys, file paths, app names or window titles. No screen recording or session replay. You can turn sharing off anytime in Settings → General.</p><div class="dlg-foot"><button type="button" class="btn sm" data-a="close">Close</button></div></div>';
    document.body.appendChild(o);
    VT.modal = true;
    var app = $("#app"); if (app) app.inert = true;
    var close = function () { o.remove(); VT.modal = false; if (app) app.inert = false; document.removeEventListener("keydown", key, true); if (anchor && anchor.isConnected) anchor.focus({ preventScroll: true }); };
    var key = function (e) { if (e.key === "Escape" || e.key === "Tab") { e.preventDefault(); e.stopPropagation(); if (e.key === "Escape") close(); } };
    document.addEventListener("keydown", key, true);
    o.addEventListener("click", function (e) { if (e.target === o || e.target.closest('[data-a="close"]')) close(); });
    $('[data-a="close"]', o).focus();
  };

  /* ---------- review controls and boot ---------- */
  var themes = ["auto", "light", "dark"], theme = 0, stamped = document.documentElement.getAttribute("data-theme");
  var clean = "";
  function restart() {
    VT.gen++;
    VT.talk = null; VT.modal = false; holding = false;
    document.body.classList.remove("holding");
    $$(".os,.pop,.toast").forEach(function (e) { e.remove(); });
    VT.state = fresh(VT.state.platform);
    $("#app").innerHTML = clean;
    VT.pill.idle();
    runEngine();
    VT.start();
    VT.paint();
  }
  VT.restart = restart;
  VT.boot = function (name, start) {
    VT.start = start;
    clean = $("#app").innerHTML;
    var ann = document.createElement("div");
    ann.className = "sr"; ann.id = "vt-announce"; ann.setAttribute("aria-live", "polite");
    document.body.appendChild(ann);
    var hint = document.createElement("div");
    hint.className = "pill-hint"; hint.id = "pill-hint"; hint.hidden = true; hint.setAttribute("role", "status");
    document.body.appendChild(hint);
    var bar = document.createElement("div");
    bar.className = "proto";
    bar.innerHTML = '<button type="button" class="tag" aria-expanded="false" title="Review controls, not part of the design">' + esc(name) + '</button><span class="more"><button type="button" data-p="restart">Restart</button><button type="button" data-p="theme">Theme: auto</button><button type="button" data-p="platform"></button><button type="button" data-p="net">Trial server: online</button><span class="note">Space stands in for any shortcut</span></span>';
    document.body.appendChild(bar);
    var platBtn = $('[data-p="platform"]', bar);
    platBtn.textContent = VT.state.platform === "mac" ? "Showing: Mac" : "Showing: Windows";
    bar.addEventListener("click", function (e) {
      var b = e.target.closest("button");
      if (!b) return;
      if (b.classList.contains("tag")) { var open = bar.classList.toggle("open"); b.setAttribute("aria-expanded", String(open)); return; }
      var p = b.dataset.p;
      if (p === "restart") restart();
      if (p === "theme") {
        theme = (theme + 1) % 3;
        b.textContent = "Theme: " + themes[theme];
        if (theme === 0) { if (stamped) document.documentElement.setAttribute("data-theme", stamped); else document.documentElement.removeAttribute("data-theme"); }
        else document.documentElement.setAttribute("data-theme", themes[theme]);
      }
      if (p === "platform") { VT.state.platform = VT.state.platform === "mac" ? "windows" : "mac"; b.textContent = VT.state.platform === "mac" ? "Showing: Mac" : "Showing: Windows"; restart(); }
      if (p === "net") { VT.failTrial = !VT.failTrial; b.textContent = "Trial server: " + (VT.failTrial ? "offline" : "online"); }
    });
    VT.pill.idle();
    runEngine();
    start();
    VT.paint();
  };
})();
