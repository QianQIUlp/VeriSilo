import {
  type EnvironmentOperation,
  type NetworkProfile,
} from "@verisilo/contracts";

export const defaultColor = "#5b5ce2";

export const defaultMihomoControllerUrl = "";

export const managedScreenChoices = [
  [1280, 800],
  [1366, 768],
  [1440, 900],
  [1920, 1080],
  [2560, 1440],
] as const;

export function screenChoiceForThisDisplay(): (typeof managedScreenChoices)[number] {
  if (typeof window === "undefined") {
    return [1280, 800];
  }
  const availWidth = window.screen.availWidth - 64;
  const availHeight = window.screen.availHeight - 64;
  const fitting = [...managedScreenChoices]
    .reverse()
    .find(([width, height]) => width <= availWidth && height <= availHeight);
  return fitting ?? [1280, 800];
}

export const managedCoreChoices = [2, 4, 6, 8, 12, 16] as const;

export function emptyNetwork(): NetworkProfile {
  return { mode: "direct", proxyRequired: false };
}

export const requiredWslCreationOperations = [
  "configureNetwork",
  "start",
  "stop",
  "health",
] satisfies EnvironmentOperation[];

export type WslCreationOption = {
  distribution: string;
  ready: boolean;
};

export type CreateMode = "standard" | "managed";
