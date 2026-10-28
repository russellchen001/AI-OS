import JSZip from "jszip";
import { createArtifactProjectArchive } from "../src/services/artifacts";
import type { ArtifactProject, ArtifactRecord } from "../src/types/artifact";
declare const process: { exit(code: number): never };

function assert(value: unknown, message: string): asserts value { if (!value) throw new Error(message); }
const project: ArtifactProject = { id: "test-project", title: "验收项目", description: "ZIP behavior fixture", favorite: false, tags: [], createdAt: 1, updatedAt: 1 };
const file: ArtifactRecord = { id: "test-file", projectId: project.id, title: "原文", filename: "note.md", path: "nested/note.md", language: "markdown", kind: "document", content: "中文与emoji✅\r\n$x^2$\nexact final line", source: "Imported", tags: ["test"], favorite: false, createdAt: 1, updatedAt: 1 };

async function run() {
  const before = JSON.stringify({ project, file });
  const zip = await JSZip.loadAsync(await createArtifactProjectArchive(project, [file]));
  assert(await zip.file(file.path)?.async("string") === file.content, "ZIP changed Unicode or line endings");
  const metadata = JSON.parse(await zip.file("ai-os-project.json")!.async("string"));
  assert(JSON.stringify(metadata) === JSON.stringify({ project, artifacts: [file] }), "ZIP metadata lost project or Artifact content");
  assert(JSON.stringify({ project, file }) === before, "export mutated stored records");
  console.log("✓ project ZIP preserves exact content and metadata without changing original records");
  for (const path of ["../escape.md", "/absolute.md", "C:\\escape.md", "\\\\server\\share.md", "nested/../../escape.md", "bad\0name", "ai-os-project.json", "ai-os-project.json/child"]) {
    let rejected = false;
    try { await createArtifactProjectArchive(project, [{ ...file, path }]); } catch { rejected = true; }
    assert(rejected, `unsafe archive path accepted: ${JSON.stringify(path)}`);
  }
  console.log("✓ traversal, absolute, control-character and reserved paths are rejected before ZIP output");
  for (const [first, second] of [["same.txt", "same.txt"], ["Readme.md", "README.md"], ["café.txt", "cafe\u0301.txt"], ["folder", "folder/note.md"]]) {
    let rejected = false;
    try { await createArtifactProjectArchive(project, [{ ...file, path: first }, { ...file, id: "second", path: second, content: "must not overwrite first" }]); } catch { rejected = true; }
    assert(rejected, "conflicting files silently overwrote content");
  }
  console.log("✓ duplicate, case/Unicode and file/directory collisions cannot silently overwrite content");
  console.log("PASS: Beta6 Artifacts archive");
}
run().catch(error => { console.error(`FAIL: Beta6 Artifacts archive — ${error.message}`); process.exit(1); });
