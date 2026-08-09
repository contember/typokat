import local from "./source.js";
import { local as Named } from "./source";
const numeric: number = local(1);
const textual: string = local("x");
local(true);
const namedNumber: number = Named(1);
const namedString: string = Named("x");
Named(true);
