Feature mocks for the browser dev server (`pnpm dev:web`). Each file exports:

```ts
import type { MockApi } from '../mock';
export function register(methods: Record<string, (p: Record<string, any>) => unknown>, api: MockApi) {
  methods['feature.method'] = (p) => ({ ... });
}
```
