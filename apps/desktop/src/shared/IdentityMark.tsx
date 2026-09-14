import type { CSSProperties } from "react";
import { identityContours } from "./identity-contours.js";

/** A local recognition aid derived only from the Silo ID, never identity evidence. */
export function IdentityMark({
  id,
  color,
  className = "",
}: {
  id: string;
  color: string;
  className?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      className={`identity-mark ${className}`}
      viewBox="0 0 240 240"
      style={{ "--identity-color": color } as CSSProperties}
    >
      {identityContours(id).map((path, layer) => {
        return (
          <path
            key={layer}
            d={path}
            fill={layer === 0 ? "currentColor" : "none"}
            fillOpacity={layer === 0 ? 0.07 : undefined}
            stroke="currentColor"
            strokeWidth={layer % 5 === 0 ? 2 : 1.1}
          />
        );
      })}
      <circle cx="130" cy="114" r="4" fill="currentColor" />
    </svg>
  );
}
