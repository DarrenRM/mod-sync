import type { Action } from "./store.svelte";
import { store } from "./store.svelte";
import { installAppUpdate } from "./updater";
import type { GameView } from "./types";

export function runAction(g: GameView, action: Action) {
  switch (action.kind) {
    case "install":
    case "update":
    case "repair":
      return store.act(g.id, (api) => api.install(g.id));
    case "play":
      return store.api.play(g.id).then(() => setTimeout(() => store.refresh(), 4000));
    case "steam":
      return store.api.getOnSteam(g.id);
    case "app":
      return installAppUpdate();
  }
}
