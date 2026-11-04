// Use the runner's short-lived artifact token, never a stored account credential.
const fs = require('node:fs');
const path = require('node:path');
const { DefaultArtifactClient } = require('@actions/artifact');
(async () => {
  const root = process.env.EDZ_ARTIFACT_DIR;
  const files = fs.readdirSync(root).map(name => path.join(root, name)).filter(file => fs.statSync(file).isFile());
  if (files.length === 0) throw new Error('No build outputs to upload');
  const result = await new DefaultArtifactClient().uploadArtifact(process.env.EDZ_ARTIFACT_NAME, files, root, { retentionDays: 30, compressionLevel: 0 });
  console.log(`Saved artifact ${result.id}: ${process.env.EDZ_ARTIFACT_NAME}`);
})().catch(error => { console.error(error.message); process.exitCode = 1; });
