export namespace globalThis {
  export const moduleLocal = 1;
}

export namespace Outer {
  export namespace globalThis {
    export const nested = 2;
  }
}

declare global {
  namespace globalThis {
    const augmented: number;
  }
}
