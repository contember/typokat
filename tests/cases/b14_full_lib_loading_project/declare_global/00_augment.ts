// External-module global augmentation collides with the library RegExp identity.
export {};

declare global {
  interface RegExp {
    b14Tag(): string;
  }

  interface WUUniqueGlobalType {
    value: number;
  }
}
