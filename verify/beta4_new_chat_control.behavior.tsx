import { renderToStaticMarkup } from "react-dom/server";
import Sidebar from "../src/components/Sidebar";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const html = renderToStaticMarkup(
  <Sidebar
    activePage="Chat"
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

const newChatButton = html.match(
  /<button[^>]*class="conversation-history-new-button"[^>]*>([\s\S]*?)<\/button>/,
);

assert(newChatButton, "new-chat control must render as a button");
assert(/aria-label="New conversation"/.test(newChatButton[0]), "accessible name missing");
assert(/title="New conversation"/.test(newChatButton[0]), "tooltip missing");
assert(/aria-hidden="true"[^>]*>\+<\/span>/.test(newChatButton[1]), "icon missing");
assert(/>New chat<\/span>/.test(newChatButton[1]), "visible label missing");

console.log("PASS Beta.4 new-chat rendered control behavior");
