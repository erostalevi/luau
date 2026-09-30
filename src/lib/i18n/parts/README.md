Feature dictionaries merged into each locale: name files `<feature>.<locale>.ts`
(e.g. `jira.en.ts`, `jira.es.ts`) with `export default { … }` using the same
nested key structure as `../en.ts`. English is required; es/pt fall back to en.
