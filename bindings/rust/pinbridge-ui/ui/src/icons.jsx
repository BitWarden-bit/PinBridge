import React from "react";

// Inline SVG icons, stroke = currentColor so they follow the theme.
// 24x24 viewBox, 1.8 stroke width — crisp at 12-14px UI size.

function I({ children, size = 14, fill = false }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={fill ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ display: "inline-block", verticalAlign: "-2px" }}
    >
      {children}
    </svg>
  );
}

export const IconPlay = (p) => (
  <I {...p} fill><polygon points="7 4.5 19 12 7 19.5" /></I>
);

export const IconPause = (p) => (
  <I {...p} fill>
    <rect x="6" y="4.5" width="4.2" height="15" rx="1" />
    <rect x="13.8" y="4.5" width="4.2" height="15" rx="1" />
  </I>
);

export const IconStepInto = (p) => (
  <I {...p}>
    <path d="M12 3v9" />
    <polyline points="8.5 9 12 12.5 15.5 9" />
    <path d="M5 20h14" />
    <path d="M5 16.5h4M15 16.5h4" opacity="0.45" />
  </I>
);

export const IconStepOver = (p) => (
  <I {...p}>
    <path d="M4 17c4-7 12-7 16 0" />
    <polyline points="16.5 14.5 20 17 16.5 19.5" />
    <circle cx="12" cy="11" r="1.6" fill="currentColor" stroke="none" />
  </I>
);

export const IconFollow = (p) => (
  <I {...p}>
    <circle cx="12" cy="12" r="7" />
    <circle cx="12" cy="12" r="2.4" fill="currentColor" stroke="none" />
    <path d="M12 2v4M12 18v4M2 12h4M18 12h4" />
  </I>
);

export const IconGo = (p) => (
  <I {...p}>
    <path d="M4 12h14" />
    <polyline points="13 6 19 12 13 18" />
  </I>
);

// Product mark: a CPU/VM chip with a sampling pulse through it.
export const IconChip = (p) => (
  <I {...p} size={18}>
    <rect x="6" y="6" width="12" height="12" rx="1.5" />
    <path d="M9 2.5v3M15 2.5v3M9 18.5v3M15 18.5v3M2.5 9h3M2.5 15h3M18.5 9h3M18.5 15h3" />
    <polyline points="8.5 12 10.5 12 11.5 9.5 13 14 14 11 15.5 11" strokeWidth="1.5" />
  </I>
);
