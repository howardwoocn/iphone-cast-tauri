import { createLucideIcon, type IconNode } from 'lucide-react';

const speaker: IconNode = [
  ['path', { d: 'M11 4 6 8H4v8h2l5 4V4Z', key: 'speaker' }],
  ['path', { d: 'M15 12h5', key: 'minus' }]
];

export const VolumeUpIcon = createLucideIcon('VolumeUp', [
  ...speaker,
  ['path', { d: 'M17.5 9.5v5', key: 'plus' }]
]);
export const VolumeDownIcon = createLucideIcon('VolumeDown', speaker);
