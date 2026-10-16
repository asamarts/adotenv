'use strict'

const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const dotenv = require('dotenv')
const packageVersion = require('dotenv/package.json').version

assert.equal(packageVersion, '18.0.8', 'The oracle requires exactly dotenv 18.0.8')

// Copy own properties into a null-prototype JSON map: inherited names must not
// disappear, and copying must not manufacture an upstream __proto__ property.
function ownValues(object) {
  const values = Object.create(null)
  for (const key of Object.getOwnPropertyNames(object).sort()) {
    values[key] = object[key]
  }
  return values
}

function prototypeLabel(object) {
  const prototype = Object.getPrototypeOf(object)
  if (prototype === null) return 'null'
  if (prototype === Object.prototype) return 'Object.prototype'
  return 'other'
}

function withEnvironment(environment, operation) {
  const original = process.env
  try {
    // Keep ambient option defaults out of every API call, including parse.
    // The loading target is a separate fresh object, not this global map.
    process.env = ownValues(environment)
    return operation()
  } finally {
    process.env = original
  }
}

const temporaryRoot = withEnvironment(Object.create(null), () => os.tmpdir())

function parseCase(testCase, fast, buffer) {
  return withEnvironment(testCase.environment, () => {
    const source = buffer ? Buffer.from(testCase.source, 'utf8') : testCase.source
    const result = dotenv.parse(source, { fast })
    return {
      values: ownValues(result),
      prototype: prototypeLabel(result),
      diagnostics_supported: false
    }
  })
}

function loadCase(testCase, override) {
  return withEnvironment(testCase.environment, () => {
    const directory = fs.mkdtempSync(path.join(temporaryRoot, 'adotenv-node-'))
    try {
      const sourcePath = path.join(directory, 'source.env')
      fs.writeFileSync(sourcePath, testCase.source, { encoding: 'utf8' })
      const environment = ownValues(testCase.environment)
      const result = dotenv.config({
        path: sourcePath,
        encoding: 'utf8',
        quiet: true,
        debug: false,
        fast: false,
        override,
        processEnv: environment
      })
      return {
        parsed: result.parsed === undefined ? null : ownValues(result.parsed),
        parsed_prototype: result.parsed === undefined ? null : prototypeLabel(result.parsed),
        environment: ownValues(environment),
        environment_prototype: prototypeLabel(environment),
        diagnostics_supported: false,
        error: result.error == null ? null : {
          name: typeof result.error.name === 'string' ? result.error.name : null,
          code: typeof result.error.code === 'string' ? result.error.code : null
        }
      }
    } finally {
      fs.rmSync(directory, { recursive: true, force: true })
    }
  })
}

function readCases() {
  const request = JSON.parse(fs.readFileSync(0, 'utf8'))
  assert.ok(request !== null && typeof request === 'object' && Array.isArray(request.cases),
    'Expected a JSON object containing cases')
  const ids = new Set()
  for (const testCase of request.cases) {
    assert.ok(testCase !== null && typeof testCase === 'object', 'Expected a case object')
    assert.equal(typeof testCase.id, 'string', 'Case id must be a string')
    assert.ok(testCase.id.length > 0 && /^[\x00-\x7f]+$/.test(testCase.id), 'Case id must be nonempty ASCII')
    assert.ok(!ids.has(testCase.id), 'Case ids must be unique')
    ids.add(testCase.id)
    assert.equal(typeof testCase.source, 'string', 'Case source must be a string')
    if (testCase.environment === undefined) testCase.environment = Object.create(null)
    assert.ok(testCase.environment !== null && typeof testCase.environment === 'object' &&
      !Array.isArray(testCase.environment), 'Case environment must be an object')
    for (const value of Object.values(testCase.environment)) {
      assert.equal(typeof value, 'string', 'Controlled environment values must be strings')
    }
  }
  return request.cases
}

const cases = readCases()
const operations = {
  'node-parse-regex-string': testCase => parseCase(testCase, false, false),
  'node-parse-regex-buffer': testCase => parseCase(testCase, false, true),
  'node-parse-fast-string': testCase => parseCase(testCase, true, false),
  'node-parse-fast-buffer': testCase => parseCase(testCase, true, true),
  'node-load-keep': testCase => loadCase(testCase, false),
  'node-load-override': testCase => loadCase(testCase, true)
}
const profiles = Object.create(null)
for (const [id, operation] of Object.entries(operations)) {
  const observations = Object.create(null)
  for (const testCase of cases) observations[testCase.id] = operation(testCase)
  profiles[id] = observations
}
process.stdout.write(JSON.stringify({
  runtime: { language: process.version, package: packageVersion },
  profiles
}) + '\n')
