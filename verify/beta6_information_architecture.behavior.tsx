import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import Sidebar from "../src/components/Sidebar";
import SettingsPage from "../src/pages/SettingsPage";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const sidebar = renderToStaticMarkup(
  <Sidebar
    activePage="AI Arena"
    onPageChange={() => undefined}
    onOpenCommandPalette={() => undefined}
    conversations={[]}
    activeConversationId=""
    onNewConversation={() => undefined}
    onSelectConversation={() => undefined}
    onRenameConversation={() => undefined}
    onDeleteConversation={() => undefined}
  />,
);

const mainNavigation = sidebar.match(/<nav[^>]*aria-label="Main navigation"[^>]*>([\s\S]*?)<\/nav>/)?.[1] ?? "";
const labels = [...mainNavigation.matchAll(/<span>([^<]+)<\/span>/g)].map((match) => match[1]);
assert(JSON.stringify(labels) === JSON.stringify(["Workspace", "My AI", "AI Lab", "Person Profiles", "Settings"]), "Sidebar does not expose exactly the five approved primary entries");
assert((mainNavigation.match(/aria-current="page"/g) ?? []).length === 1, "A legacy AI Lab route must activate exactly one primary entry");

const settings = renderToStaticMarkup(
  <SettingsPage
    settings={{ refreshInterval: 5000, openClawUrl: "", theme: "dark", logLineLimit: 100 }}
    cardStyle={{}}
    onUpdateSetting={() => undefined}
    onReset={() => undefined}
    onOpenSection={() => undefined}
  />,
);
assert(settings.includes("Advanced · Skills"), "Skills are not reachable from Settings advanced controls");

console.log("PASS: Beta 6 information architecture behavior");
