// The search over every page: each section whose page title, heading or text
// has every word typed, those found in a heading first. IMAGO_INDEX comes
// from search-index.js: [page, page title, anchor, heading, text] per section.
(function () {
  var input = document.getElementById("search");
  var box = document.getElementById("results");
  var index = window.IMAGO_INDEX || [];
  var on = -1;

  function esc(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }
  function marked(s, words) {
    var out = esc(s);
    words.forEach(function (w) {
      var re = new RegExp("(" + w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + ")", "gi");
      out = out.replace(re, "<mark>$1</mark>");
    });
    return out;
  }
  function snippet(text, word) {
    var at = text.toLowerCase().indexOf(word);
    if (at < 0) return text.slice(0, 140);
    var from = Math.max(0, at - 60);
    return (from > 0 ? "…" : "") + text.slice(from, from + 160) + (from + 160 < text.length ? "…" : "");
  }

  function find(q) {
    var words = q.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    var hits = [];
    index.forEach(function (e) {
      var title = e[1].toLowerCase(), heading = e[3].toLowerCase(), text = e[4].toLowerCase();
      var score = 0;
      for (var i = 0; i < words.length; i++) {
        var w = words[i], inH = heading.indexOf(w) >= 0, inT = title.indexOf(w) >= 0, inX = text.indexOf(w) >= 0;
        if (!inH && !inT && !inX) return;
        score += (inH ? 10 : 0) + (inT ? 4 : 0) + (inX ? 1 : 0);
      }
      hits.push([score, e]);
    });
    hits.sort(function (a, b) { return b[0] - a[0]; });
    return hits.slice(0, 30).map(function (h) { return h[1]; });
  }

  function show() {
    var q = input.value.trim();
    on = -1;
    if (!q) { box.hidden = true; return; }
    var words = q.toLowerCase().split(/\s+/).filter(Boolean);
    var hits = find(q);
    if (!hits.length) {
      box.innerHTML = "<p>Nothing found for “" + esc(q) + "”</p>";
    } else {
      box.innerHTML = hits.map(function (e) {
        var href = e[0] + (e[2] ? "#" + e[2] : "");
        var heading = e[3] || e[1];
        return '<a href="' + href + '"><b>' + marked(heading, words) + "</b>" +
          (e[3] ? "<small>" + esc(e[1]) + "</small>" : "") +
          "<span>" + marked(snippet(e[4], words[0]), words) + "</span></a>";
      }).join("");
    }
    box.hidden = false;
  }

  function move(by) {
    var links = box.querySelectorAll("a");
    if (!links.length) return;
    if (on >= 0) links[on].classList.remove("on");
    on = (on + by + links.length) % links.length;
    links[on].classList.add("on");
    links[on].scrollIntoView({ block: "nearest" });
  }

  input.addEventListener("input", show);
  input.addEventListener("focus", show);
  input.addEventListener("keydown", function (ev) {
    if (ev.key === "ArrowDown") { move(1); ev.preventDefault(); }
    else if (ev.key === "ArrowUp") { move(-1); ev.preventDefault(); }
    else if (ev.key === "Enter") {
      var links = box.querySelectorAll("a");
      var go = links[on >= 0 ? on : 0];
      if (go) window.location.href = go.getAttribute("href");
    } else if (ev.key === "Escape") { input.value = ""; box.hidden = true; input.blur(); }
  });
  document.addEventListener("keydown", function (ev) {
    if (ev.key === "/" && document.activeElement !== input) { ev.preventDefault(); input.focus(); }
  });
  document.addEventListener("click", function (ev) {
    if (!ev.target.closest(".find")) box.hidden = true;
    if (ev.target.closest(".menu")) document.body.classList.toggle("open");
    else if (!ev.target.closest(".side")) document.body.classList.remove("open");
  });

  // The page's own entry kept in view in the contents.
  var side = document.querySelector(".side"), here = document.querySelector(".side a.here");
  if (side && here) side.scrollTop = here.offsetTop - side.clientHeight / 2;
})();
