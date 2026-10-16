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
for (const url of [
  "https://pan.baidu.com/s/1SafeShare_1",
  "https://pan.quark.cn/s/SafeShare_2",
  "https://www.alipan.com/s/SafeShare_3",
  "https://drive.uc.cn/s/SafeShare_4",
  "https://www.123pan.com/s/SafeShare_5",
  "https://caiyun.139.com/m/SafeShare_6",
  "https://cloud.189.cn/t/SafeShare_7",
  "https://pan.xunlei.com/s/SafeShare_8",
  "https://www.guangyapan.com/s/SafeShare_9",
]) {
  equal(supportedCloudDriveShareLink(url), url, `supported share was not detected: ${url}`);
}
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
  componentId: "ai-os-cloud-drive-mcp",
  name: "AI-OS Cloud Drive MCP",
  pinnedVersion: "0.1.0",
  license: "Apache-2.0",
  sourceUrl: "https://github.com/russellchen001/AI-OS",
  permissions: ["Run as an isolated local stdio MCP process"],
  installed: false,
  integrityVerified: false,
  registered: false,
  updateAvailable: false,
  releaseConfigured: false,
  message: "Not installed",
});

matches(consent, /AI-OS Cloud Drive MCP 0\.1\.0/, "fixed version is missing");
matches(consent, /Apache-2\.0/, "license is missing");
matches(consent, /github\.com\/russellchen001\/AI-OS/, "source URL is missing");
matches(consent, /sign-in, CAPTCHA or file access approval/, "provider access warning is missing");
matches(consent, /does not promise anonymous access or support for every link/, "anonymous-access limit is missing");

console.log("AI-OS Cloud Drive MCP frontend behavior passed");
