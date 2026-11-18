const loadButton = document.querySelector("[data-load-demo]");
const demoStage = document.querySelector("[data-demo-stage]");

function mountWorkbench() {
  if (!demoStage || demoStage.querySelector("iframe")) return;
  const frame = document.createElement("iframe");
  frame.src = "/demo/";
  frame.title = "Midori interactive workbench";
  frame.allow = "fullscreen";
  demoStage.append(frame);
  frame.focus();
}

loadButton?.addEventListener("click", mountWorkbench);

// A link with ?run scrolls straight to the workbench and boots it immediately.
if (new URLSearchParams(window.location.search).has("run")) {
  document.querySelector("#demo")?.scrollIntoView();
  mountWorkbench();
}
