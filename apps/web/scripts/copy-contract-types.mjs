// Copy the generated OpenAPI types into src/api/v3.d.ts (single source of
// truth: packages/contract/openapi.yaml). Full flow in apps/web/README.md.
import { copyFileSync, mkdirSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '..') // apps/web
const source = join(root, '..', '..', 'packages', 'contract', 'generated', 'v3.d.ts')
const targetDir = join(root, 'src', 'api')
const target = join(targetDir, 'v3.d.ts')

if (!existsSync(source)) {
  console.error(`[gen:api] generated contract file not found: ${source}`)
  console.error('  run first: cd packages/contract && npm i && npm run gen')
  process.exit(1)
}

mkdirSync(targetDir, { recursive: true })
copyFileSync(source, target)
console.log(`[gen:api] copied ${source} → ${target}`)
