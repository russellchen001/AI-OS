import { classifyProviderError, canRouteToProvider, healthLabel } from "../src/services/providerRuntime";
import { describeChatTaskError } from "../src/services/tasks";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const quota = classifyProviderError("HTTP 429 RESOURCE_EXHAUSTED: quota exceeded");
assert(quota.health === "quota" && !canRouteToProvider(quota), "Resource limits must be non-routable quota failures");

const oauthBlock = classifyProviderError("HTTP 403 PERMISSION_DENIED: app is not verified");
assert(oauthBlock.health === "account-blocked", "OAuth review blocks must not be reported as invalid API keys");
assert(healthLabel(oauthBlock) === "Account access blocked", "OAuth block label is unclear");

const invalidKey = classifyProviderError("HTTP 401 invalid_api_key");
assert(invalidKey.health === "invalid-key", "Locally replaceable invalid credentials must remain distinct");

assert(
  describeChatTaskError(new Error("The selected Provider returned HTTP 403 (PERMISSION_DENIED)"), false).includes("outside AI-OS"),
  "Google OAuth review guidance does not identify the external blocker",
);
assert(
  describeChatTaskError(new Error("The selected Provider returned HTTP 429 (RESOURCE_EXHAUSTED)"), false).includes("quota"),
  "Provider resource-limit guidance is missing",
);

console.log("PASS: Provider blocker classification behavior");
