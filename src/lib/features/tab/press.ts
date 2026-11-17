/**
 * A press on the edge tab stays a click only while the pointer never leaves the system drag
 * tolerance, so dragging away and back before releasing does nothing. Physical pixels throughout.
 */
export class Press {
  #x: number;
  #y: number;
  #dragged = false;

  constructor(x: number, y: number) {
    this.#x = x;
    this.#y = y;
  }

  moved(x: number, y: number, [slopX, slopY]: readonly [number, number]) {
    this.#dragged ||= Math.abs(x - this.#x) > slopX || Math.abs(y - this.#y) > slopY;
  }

  isClick(x: number, y: number, slop: readonly [number, number]) {
    this.moved(x, y, slop);
    return !this.#dragged;
  }
}
