import { Status } from "./source.js";

const whole: Status = Status.Ready;
const member: Status.Done = Status.Done;
const label: string = Status.label;
const payload: Status.Payload = { status: Status.Ready };
const wrong: Status.Done = Status.Ready; // error[TK2322]
