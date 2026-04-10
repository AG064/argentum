/**
 * Live Canvas Feature
 *
 * Real-time collaborative drawing and visualization canvas.
 * Supports shapes, freehand drawing, text, and image overlays.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Live Canvas configuration */
export interface LiveCanvasConfig {
    enabled: boolean;
    maxWidth: number;
    maxHeight: number;
    maxLayers: number;
    defaultBackground: string;
}
/** Canvas element types */
export type CanvasElementType = 'rectangle' | 'circle' | 'line' | 'path' | 'text' | 'image';
/** Canvas element */
export interface CanvasElement {
    id: string;
    type: CanvasElementType;
    x: number;
    y: number;
    width?: number;
    height?: number;
    points?: Array<{
        x: number;
        y: number;
    }>;
    text?: string;
    src?: string;
    style: {
        fill?: string;
        stroke?: string;
        strokeWidth?: number;
        fontSize?: number;
        fontFamily?: string;
        opacity?: number;
    };
    layer: number;
    locked: boolean;
    createdAt: number;
}
/** Canvas state */
export interface Canvas {
    id: string;
    name: string;
    width: number;
    height: number;
    background: string;
    elements: Map<string, CanvasElement>;
    layers: string[];
    collaborators: Set<string>;
    createdAt: number;
    updatedAt: number;
}
/** Canvas operation */
export interface CanvasOperation {
    type: 'add' | 'update' | 'delete' | 'move' | 'resize';
    elementId: string;
    data?: Partial<CanvasElement>;
    userId: string;
    timestamp: number;
}
/**
 * Live Canvas feature — real-time collaborative drawing canvas.
 *
 * Provides a shared canvas for drawing, annotating, and visualizing
 * data with multi-user collaboration support.
 */
declare class LiveCanvasFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private canvases;
    private operationListeners;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Create a new canvas */
    createCanvas(name: string, width?: number, height?: number): Canvas;
    /** Get a canvas by ID */
    getCanvas(id: string): Canvas | undefined;
    /** Add an element to canvas */
    addElement(canvasId: string, element: Omit<CanvasElement, 'id' | 'createdAt'>): CanvasElement | null;
    /** Update an element */
    updateElement(canvasId: string, elementId: string, updates: Partial<CanvasElement>): boolean;
    /** Delete an element */
    deleteElement(canvasId: string, elementId: string): boolean;
    /** Subscribe to canvas operations */
    onOperation(canvasId: string, handler: (op: CanvasOperation) => void): () => void;
    /** Emit operation to listeners */
    private emitOperation;
    /** Export canvas as JSON */
    exportCanvas(canvasId: string): string | null;
}
declare const _default: LiveCanvasFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map