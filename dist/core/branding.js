"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ARGENTUM_BANNER = void 0;
exports.formatArgentumBanner = formatArgentumBanner;
exports.ARGENTUM_BANNER = `
    AAAAA    RRRRR     GGGGG   EEEEEEE  N     N  TTTTTTT  U     U  M     M
   A     A   R    R   G        E        NN    N     T     U     U  MM   MM
   AAAAAAA   RRRRR    G  GGG   EEEEE    N N   N     T     U     U  M M M M
   A     A   R   R    G    G   E        N  N  N     T     U     U  M  M  M
   A     A   R    R    GGGG    EEEEEEE  N    NN     T      UUUUU   M     M

                                ARGENTUM`;
function formatArgentumBanner(version) {
    return `${exports.ARGENTUM_BANNER}

     Argentum  |  Modular AI Agent Framework  |  v${version}`;
}
