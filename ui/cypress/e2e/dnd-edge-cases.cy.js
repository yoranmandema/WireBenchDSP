// Drag-and-drop edge cases, the touch fallback, and the keyboard alternative.
//
// The happy path (drop between blocks, at the end, into an empty channel) is
// covered in `chain.cy.js`. This spec covers what happens when the drop is not
// well formed, when it lands nowhere, and the two non-mouse routes.

/** Build a DataTransfer carrying a block payload. */
function blockTransfer(win, payload) {
  const dt = new win.DataTransfer();
  dt.setData("application/x-block-type", JSON.stringify(payload));
  return dt;
}

/** Dispatch a drag event on `el` with `dt`. */
function drag(el, type, dt, opts = {}) {
  el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt, ...opts }));
}

describe("drop edge cases", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("ignores a drop with no payload and warns", () => {
    cy.window().then((win) => {
      const warn = cy.stub(win.console, "warn");
      const empty = new win.DataTransfer();
      cy.get(".channel").eq(0).then(($ch) => {
        drag($ch[0], "drop", empty, { clientX: 200, clientY: 200 });
      });
      cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
      cy.then(() => {
        expect(warn.calledWithMatch(/no block payload/)).to.be.true;
      });
    });
  });

  it("ignores a drop whose payload is not a block and warns", () => {
    cy.window().then((win) => {
      const warn = cy.stub(win.console, "warn");
      // Plain text is not a block payload.
      const dt = new win.DataTransfer();
      dt.setData("text/plain", "just some text");
      cy.get(".channel").eq(0).then(($ch) => {
        drag($ch[0], "drop", dt, { clientX: 200, clientY: 200 });
      });
      cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
      cy.then(() => {
        expect(warn.calledWithMatch(/no block payload/)).to.be.true;
      });
    });
  });

  it("ignores a drop whose block type id is unknown and warns", () => {
    cy.window().then((win) => {
      const warn = cy.stub(win.console, "warn");
      const dt = blockTransfer(win, { type: "no.such.block", name: "Mystery" });
      cy.get(".channel").eq(0).then(($ch) => {
        drag($ch[0], "drop", dt, { clientX: 200, clientY: 200 });
      });
      cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
      cy.then(() => {
        expect(warn.calledWithMatch(/unknown block type/)).to.be.true;
      });
    });
  });

  it("clears the dragging marker on dragend after a refused drop", () => {
    cy.window().then((win) => {
      const dt = blockTransfer(win, { type: "no.such.block", name: "Mystery" });
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        drag(row, "dragstart", dt, { clientX: 100, clientY: 100 });
        expect(row.classList.contains("dragging")).to.be.true;
        cy.get(".channel").eq(0).then(($ch) => {
          drag($ch[0], "drop", dt, { clientX: 200, clientY: 200 });
          expect(row.classList.contains("dragging")).to.be.true;
          drag(row, "dragend", dt);
          expect(row.classList.contains("dragging")).to.be.false;
        });
      });
      cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
    });
  });

  it("clears the dragging marker on dragend after a cancelled drag", () => {
    // No drop at all: the user releases outside every channel.
    cy.window().then((win) => {
      const dt = blockTransfer(win, { type: "time.reverb", name: "Reverb" });
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        drag(row, "dragstart", dt, { clientX: 100, clientY: 100 });
        expect(row.classList.contains("dragging")).to.be.true;
        drag(row, "dragend", dt);
        expect(row.classList.contains("dragging")).to.be.false;
      });
      cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
    });
  });

  it("does nothing when a drag is dropped outside every channel", () => {
    cy.window().then((win) => {
      const dt = blockTransfer(win, { type: "time.reverb", name: "Reverb" });
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        drag(row, "dragstart", dt);
        cy.get(".editor").then(($editor) => {
          drag($editor[0], "drop", dt, { clientX: 20, clientY: 20 });
        });
        drag(row, "dragend", dt);
      });
      cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
      cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
    });
  });
});

describe("touch fallback", () => {
  beforeEach(() => {
    cy.boot();
  });

  /** Long-press a rack row, then drag to a channel's centre and release. */
  function longPressDrop(channelIndex) {
    cy.contains(".rack-item", /^Reverb$/).then(($row) => {
      const row = $row[0];
      const win = row.ownerDocument.defaultView;
      row.dispatchEvent(new win.PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 }));
      // Hold still past the 350 ms threshold; the timer is what starts the drag.
      cy.wait(450, { log: false }).then(() => {
        cy.get(".channel").eq(channelIndex).then(($ch) => {
          const rect = $ch[0].getBoundingClientRect();
          const tx = Math.round(rect.left + rect.width / 2);
          const ty = Math.round(rect.top + rect.height / 2);
          win.dispatchEvent(new win.PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: tx, clientY: ty }));
          win.dispatchEvent(new win.PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: tx, clientY: ty }));
        });
      });
    });
  }

  it("inserts a block after a long press and drag into a channel", () => {
    longPressDrop(2);
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Reverb");
  });

  it("inserts into a populated channel at the slot under the finger", () => {
    cy.contains(".rack-item", /^Reverb$/).then(($row) => {
      const row = $row[0];
      const win = row.ownerDocument.defaultView;
      row.dispatchEvent(new win.PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 }));
      cy.wait(450, { log: false }).then(() => {
        cy.get(".channel").eq(0).find(".block-card").eq(0).then(($a) => {
          cy.get(".channel").eq(0).find(".block-card").eq(1).then(($b) => {
            const ra = $a[0].getBoundingClientRect();
            const rb = $b[0].getBoundingClientRect();
            const tx = Math.round((ra.right + rb.left) / 2);
            const ty = Math.round(ra.top + ra.height / 2);
            win.dispatchEvent(new win.PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: tx, clientY: ty }));
            win.dispatchEvent(new win.PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: tx, clientY: ty }));
          });
        });
      });
    });
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 4);
    cy.get(".channel").eq(0).find(".block-card").eq(1).should("contain", "Reverb");
  });

  it("does nothing when the touch drag is released outside every channel", () => {
    cy.contains(".rack-item", /^Reverb$/).then(($row) => {
      const row = $row[0];
      const win = row.ownerDocument.defaultView;
      row.dispatchEvent(new win.PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 }));
      cy.wait(450, { log: false }).then(() => {
        win.dispatchEvent(new win.PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 20, clientY: 20 }));
        win.dispatchEvent(new win.PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 20, clientY: 20 }));
      });
    });
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
  });

  it("does not start a drag when the press moves before the long press", () => {
    cy.contains(".rack-item", /^Reverb$/).then(($row) => {
      const row = $row[0];
      row.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 }));
      // Move out of the 10 px slop well before 350 ms: a scroll, not a drag.
      row.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 160, clientY: 160 }));
      cy.wait(500, { log: false }).then(() => {
        window.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 1, pointerType: "touch", clientX: 160, clientY: 160 }));
      });
    });
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
  });
});

describe("keyboard insert", () => {
  beforeEach(() => {
    cy.boot();
  });

  /** Select a rack row with Enter/Space. */
  function selectRow(name) {
    cy.contains(".rack-item", new RegExp(`^${name}$`)).then(($row) => {
      const el = $row[0];
      const win = el.ownerDocument.defaultView;
      el.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    });
  }

  it("inserts the selected block at the end of a channel", () => {
    cy.get(".channel").eq(0).find(".add-selected").should("be.disabled");
    selectRow("Reverb");

    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
    cy.get(".channel").eq(0).find(".add-selected").should("not.be.disabled").click();

    cy.get(".channel").eq(0).find(".block-card").should("have.length", 4);
    cy.get(".channel").eq(0).find(".block-card").eq(3).should("contain", "Reverb");
    // The choice is consumed, so the button is disabled again.
    cy.get(".channel").eq(0).find(".add-selected").should("be.disabled");
  });

  it("inserts into an empty channel, at its only slot", () => {
    selectRow("Overdrive");
    cy.get(".channel").eq(2).find(".add-selected").click();

    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Overdrive");
    cy.get(".channel").eq(2).should("not.have.class", "is-empty");
  });

  it("marks the selected row so the choice is visible", () => {
    selectRow("Delay");
    cy.contains(".rack-item", /^Delay$/).should("have.attr", "aria-pressed", "true");
  });

  it("does nothing when the channel's Add is clicked with nothing selected", () => {
    cy.get(".channel").eq(2).find(".add-selected").should("be.disabled");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
  });

  it("keeps the chain inside the window when a channel is full", () => {
    // The strip scrolls its own overflow, so a row of cards must never widen
    // the grid track: the channel grows past the viewport and the whole chain
    // panel scrolls horizontally instead. Regression test for the keyboard-insert
    // button, which pushed the row past the strip and made the grid 820px wide
    // at an 800px window.
    cy.viewport(800, 600);
    cy.get(".channel").eq(0).find(".block-card").should("have.length.at.least", 3);

    cy.get(".chain-panel").should(($panel) => {
      expect($panel[0].scrollWidth).to.be.at.most($panel[0].clientWidth);
    });
    cy.get(".chain-scroll").should(($grid) => {
      expect($grid[0].scrollWidth).to.be.at.most($grid[0].clientWidth);
    });
    cy.get(".channel").eq(0).find(".strip").should(($strip) => {
      // The strip itself is the scroller now.
      expect($strip[0].scrollWidth).to.be.greaterThan($strip[0].clientWidth);
    });
    cy.get(".channel").eq(0).should(($ch) => {
      // The channel itself is inside the window. `window` here is the Cypress
      // runner, not the app under test, so read the width from the AUT document.
      const win = $ch[0].ownerDocument.defaultView;
      expect($ch[0].getBoundingClientRect().right).to.be.at.most(win.innerWidth + 1);
    });
    // The strip scrolls its own overflow: its content (the trailing Out node)
    // runs past its visible box, while the box itself stays inside the window.
    cy.get(".channel").eq(0).find(".strip").then(($strip) => {
      const strip = $strip[0];
      const rect = strip.getBoundingClientRect();
      const out = strip.querySelector(".io.out").getBoundingClientRect();
      expect(out.right).to.be.greaterThan(rect.right);
      expect(out.right).to.be.at.most(strip.ownerDocument.defaultView.innerWidth + 1);
    });
  });
});
