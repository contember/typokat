export {};

declare global {
  interface RegExp {
    b103ContinuationTag(): string;
  }

  interface B103MixedFreshContinuation {
    count: number;
  }
}
