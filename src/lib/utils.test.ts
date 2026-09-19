import { expect, it } from "vitest";
import { cn } from "./utils";

it("cn 合并类名，条件类名与冲突类名正确处理", () => {
  expect(cn("a", "b")).toBe("a b");
  expect(cn("a", false && "b", "c")).toBe("a c");
  // tailwind-merge：同一属性的后者优先保留
  expect(cn("p-2", "p-4")).toBe("p-4");
  expect(cn("px-2 py-1", "px-4")).toBe("py-1 px-4");
});
