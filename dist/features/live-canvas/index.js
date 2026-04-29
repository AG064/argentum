"use strict";
/**
 * Live Canvas Feature
 *
 * Real-time collaborative drawing and visualization canvas.
 * Supports shapes, freehand drawing, text, and image overlays.
 */
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * Live Canvas feature — real-time collaborative drawing canvas.
 *
 * Provides a shared canvas for drawing, annotating, and visualizing
 * data with multi-user collaboration support.
 */
class LiveCanvasFeature {
    meta = {
        name: 'live-canvas',
        version: '0.0.4',
        description: 'Real-time collaborative drawing and visualization canvas',
        dependencies: [],
    };
    config = {
        enabled: false,
        maxWidth: 1920,
        maxHeight: 1080,
        maxLayers: 10,
        defaultBackground: '#ffffff',
    };
    ctx;
    canvases = new Map();
    operationListeners = new Map();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.ctx.logger.info('Live Canvas active');
    }
    async stop() {
        this.canvases.clear();
        this.operationListeners.clear();
    }
    async healthCheck() {
        return {
            healthy: true,
            details: { canvases: this.canvases.size },
        };
    }
    /** Create a new canvas */
    createCanvas(name, width, height) {
        const id = `canvas_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const canvas = {
            id,
            name,
            width: width ?? this.config.maxWidth,
            height: height ?? this.config.maxHeight,
            background: this.config.defaultBackground,
            elements: new Map(),
            layers: ['default'],
            collaborators: new Set(),
            createdAt: Date.now(),
            updatedAt: Date.now(),
        };
        this.canvases.set(id, canvas);
        return canvas;
    }
    /** Get a canvas by ID */
    getCanvas(id) {
        return this.canvases.get(id);
    }
    /** Add an element to canvas */
    addElement(canvasId, element) {
        const canvas = this.canvases.get(canvasId);
        if (!canvas)
            return null;
        const fullElement = {
            ...element,
            id: `el_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
            createdAt: Date.now(),
        };
        canvas.elements.set(fullElement.id, fullElement);
        canvas.updatedAt = Date.now();
        this.emitOperation(canvasId, {
            type: 'add',
            elementId: fullElement.id,
            data: fullElement,
            userId: 'system',
            timestamp: Date.now(),
        });
        return fullElement;
    }
    /** Update an element */
    updateElement(canvasId, elementId, updates) {
        const canvas = this.canvases.get(canvasId);
        if (!canvas)
            return false;
        const element = canvas.elements.get(elementId);
        if (!element)
            return false;
        Object.assign(element, updates);
        canvas.updatedAt = Date.now();
        this.emitOperation(canvasId, {
            type: 'update',
            elementId,
            data: updates,
            userId: 'system',
            timestamp: Date.now(),
        });
        return true;
    }
    /** Delete an element */
    deleteElement(canvasId, elementId) {
        const canvas = this.canvases.get(canvasId);
        if (!canvas)
            return false;
        const deleted = canvas.elements.delete(elementId);
        if (deleted) {
            canvas.updatedAt = Date.now();
            this.emitOperation(canvasId, {
                type: 'delete',
                elementId,
                userId: 'system',
                timestamp: Date.now(),
            });
        }
        return deleted;
    }
    /** Subscribe to canvas operations */
    onOperation(canvasId, handler) {
        const listeners = this.operationListeners.get(canvasId) ?? new Set();
        listeners.add(handler);
        this.operationListeners.set(canvasId, listeners);
        return () => listeners.delete(handler);
    }
    /** Emit operation to listeners */
    emitOperation(canvasId, op) {
        const listeners = this.operationListeners.get(canvasId);
        if (!listeners)
            return;
        for (const handler of listeners) {
            try {
                handler(op);
            }
            catch {
                // Swallow listener errors
            }
        }
    }
    /** Export canvas as JSON */
    exportCanvas(canvasId) {
        const canvas = this.canvases.get(canvasId);
        if (!canvas)
            return null;
        return JSON.stringify({
            ...canvas,
            elements: Array.from(canvas.elements.entries()),
            collaborators: Array.from(canvas.collaborators),
        });
    }
}
exports.default = new LiveCanvasFeature();
//# sourceMappingURL=index.js.map