export enum Status {
  Ready = "ready",
  Done = "done",
}

export namespace Status {
  export const label = "status";

  export interface Payload {
    status: Status;
  }
}
