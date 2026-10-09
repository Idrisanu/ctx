// Typed terminal demo: the token-limit scenario.
document.body.classList.add("js");
const SCRIPT = [
  ["p", "$ ctx init --agent claude-code"],
  ["c", "Refreshed CTX in ./ctx-demo2/.ctx\nAGENTS.md + CLAUDE.md updated with CTX note instructions.\ngit post-commit hook installed (auto-checkpoint)."],
  ["p", "$ # ...Claude builds the landing page for an hour..."],
  ["p", "$ npm run build"],
  ["c", "✓ Compiled successfully\n✓ 3 static routes generated"],
  ["p", "$ git add -A && git commit -m \"claude: landing page + docs section\""],
  ["c", "[master 9f3ac21] claude: landing page + docs section"],
  ["w", "⚠ API Error: You have exhausted your daily quota on this model."],
  ["p", "$ # tokens dead mid-milestone. New agent, same folder:"],
  ["p", "$ ctx resume"],
  ["c", "Context prepared for 'claude':\n./.ctx/handoffs/claude.md"],
  ["p", "$ cat .ctx/handoffs/claude.md"],
  ["c", "# Project Context (from CTX)\n\n**Objective:** Ship the CTX landing page\n\n## Completed\n- Landing scaffold + hero\n- Docs section + footer\n\n## Current work\n- Contact form (route done, validation half-written)\n\n## Next action\nFinish contact-form validation in Contact.tsx\n\n---\nSources: project/git state + ingested AI session + AI cooperative note"],
  ["p", "$ # paste into the next agent → it continues at Contact.tsx. No re-explaining."],
];

const body = document.getElementById("term-body");
let timers = [];

function play() {
  timers.forEach(clearTimeout);
  timers = [];
  body.innerHTML = "";
  let t = 300;
  for (const [kind, text] of SCRIPT) {
    if (kind === "p") {
      timers.push(setTimeout(() => {
        const line = document.createElement("div");
        line.className = "p";
        body.appendChild(line);
        let i = 0;
        const iv = setInterval(() => {
          line.textContent = text.slice(0, ++i);
          if (i >= text.length) clearInterval(iv);
        }, 18);
        timers.push(iv);
        body.scrollTop = body.scrollHeight;
      }, t));
      t += text.length * 18 + 500;
    } else {
      timers.push(setTimeout(() => {
        const line = document.createElement("div");
        line.className = kind;
        line.textContent = text;
        body.appendChild(line);
        body.scrollTop = body.scrollHeight;
      }, t));
      t += 700;
    }
  }
}

document.getElementById("replay").addEventListener("click", play);

// Scroll-reveal (respects reduced motion via CSS)
const io = new IntersectionObserver((entries) => {
  entries.forEach(e => { if (e.isIntersecting) { e.target.classList.add("visible"); io.unobserve(e.target); } });
}, { threshold: 0.12 });
document.querySelectorAll(".reveal").forEach(el => io.observe(el));

// Install tabs
const tabs = document.querySelectorAll(".tabs button");
tabs.forEach(btn => btn.addEventListener("click", () => {
  tabs.forEach(b => b.classList.remove("active"));
  btn.classList.add("active");
  document.querySelectorAll(".tabbody pre").forEach(p => p.hidden = true);
  document.getElementById("install-" + btn.dataset.tab).hidden = false;
}));

// Copy buttons
function copyText(text, btn) {
  navigator.clipboard.writeText(text).then(() => {
    const old = btn.textContent;
    btn.textContent = "Copied!";
    setTimeout(() => btn.textContent = old, 1200);
  });
}
document.querySelectorAll("[data-copy]").forEach(btn =>
  btn.addEventListener("click", () =>
    copyText(document.getElementById(btn.dataset.copy).textContent.trim(), btn)));
document.querySelector("[data-copy-current]").addEventListener("click", (e) => {
  const visible = [...document.querySelectorAll(".tabbody pre")].find(p => !p.hidden);
  copyText(visible.textContent.trim(), e.target);
});

play();
