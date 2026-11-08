// Next.js stand-in: next/font/google downloads fonts at build time; gpuijs uses installed ones, so
// each font is empty and the theme's own font stacks apply.
const font = () => ({ className: '', variable: '', style: { fontFamily: '' } });
const fonts: Record<string, typeof font> = new Proxy({}, { get: () => font });
export const { Architects_Daughter, DM_Sans, Fira_Code, Geist, Geist_Mono, Google_Sans_Flex, Instrument_Sans, Inter, JetBrains_Mono, Merriweather, Mulish, Playfair_Display, Noto_Sans_Mono, Outfit, Source_Code_Pro, Space_Mono } = fonts;
