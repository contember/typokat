// Namespace closure keeps string-literal ambient external modules explicit under backlog 15.
declare module "typokat-ambient-external" { // incomplete[decl/module-declaration/self]: namespace/module declaration has an unmodeled value surface
  export interface Payload {
    value: string;
  }
  export const payload: Payload;
}
