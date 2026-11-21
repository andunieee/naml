// renders ```mermaid code blocks as diagrams, in the theme's light or dark colors. if mermaid
// can't be loaded this module never runs and the diagram source stays readable as code
import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs";

const diagrams = [...document.querySelectorAll('pre > code[data-lang="mermaid"]')].map((code) => {
  const div = document.createElement("div");
  div.className = "diagram";
  div.dataset.source = code.textContent;
  code.parentElement.replaceWith(div);
  return div;
});

let generation = 0;

async function render() {
  const dark = document.body.classList.contains("dark");
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: dark ? "dark" : "neutral",
    fontFamily: "inherit",
  });
  generation += 1;
  for (const [i, div] of diagrams.entries()) {
    try {
      const { svg } = await mermaid.render(`diagram-${generation}-${i}`, div.dataset.source);
      div.innerHTML = svg;
    } catch (err) {
      const pre = document.createElement("pre");
      pre.textContent = div.dataset.source;
      div.replaceChildren(pre);
      console.error("diagram failed to render", err);
    }
  }
}

if (diagrams.length > 0) {
  render();
  // the theme's own handler flips the "dark" class first
  document.getElementById("mode")?.addEventListener("click", render);
}
