import {
  linkSwiftConsentMessage,
  supportedCloudDriveShareLink,
} from "../src/services/linkswiftMcp";

function equal(actual: unknown, expected: unknown, message: string) {
  if (actual !== expected) throw new Error(`${message}: ${String(actual)}`);
}

function matches(value: string, pattern: RegExp, message: string) {
  if (!pattern.test(value)) throw new Error(message);
}

equal(
  supportedCloudDriveShareLink("请下载 https://pan.quark.cn/s/authorized-file 提取码 1234"),
  "https://pan.quark.cn/s/authorized-file",
  "supported Quark link was not detected",
);
equal(
  supportedCloudDriveShareLink("https://pan.baidu.com.evil.example/s/not-supported"),
  undefined,
  "lookalike host was accepted",
);
equal(
  supportedCloudDriveShareLink("http://pan.quark.cn/s/insecure"),
  undefined,
  "insecure link was accepted",
);

const consent = linkSwiftConsentMessage({
  componentId: "linkswift-mcp",
  name: "LinkSwift MCP",
  pinnedVersion: "0.1.0",
  license: "AGPL-3.0-or-later",
  sourceUrl: "https://github.com/russellchen001/linkswift-mcp",
  permissions: ["Run as an isolated local stdio MCP process"],
  installed: false,
  integrityVerified: false,
  registered: false,
  updateAvailable: false,
  releaseConfigured: false,
  message: "Not installed",
});

matches(consent, /LinkSwift MCP 0\.1\.0/, "fixed version is missing");
matches(consent, /AGPL-3\.0-or-later/, "license is missing");
matches(consent, /github\.com\/russellchen001\/linkswift-mcp/, "source URL is missing");
matches(consent, /sign-in, CAPTCHA or file access approval/, "provider access warning is missing");
matches(consent, /does not promise anonymous access or support for every link/, "anonymous-access limit is missing");

console.log("LinkSwift MCP frontend behavior passed");
