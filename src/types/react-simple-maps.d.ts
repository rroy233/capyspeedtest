declare module "react-simple-maps" {
  import React from "react";

  export interface ComposableMapProps {
    width?: number;
    height?: number;
    projection?: string;
    projectionConfig?: {
      scale?: number;
      center?: [number, number];
      rotate?: [number, number, number];
      parallels?: [number, number];
    };
    style?: React.CSSProperties;
    className?: string;
    children?: React.ReactNode;
  }

  export interface GeographiesProps {
    geography: string | object | object[];
    children: (props: {
      geographies: Geography[];
      outline?: object;
      borders?: object[];
      path?: string;
      projection?: Function;
    }) => React.ReactNode;
    parseGeographies?: (geographies: object[]) => object[];
    className?: string;
  }

  export interface Geography {
    rsmKey: string;
    id: string | number;
    svgPath?: string;
    properties?: Record<string, unknown>;
    geometry?: object;
    type?: string;
    coordinates?: number[][][];
  }

  export interface GeographyProps {
    key?: string;
    geography: Geography;
    style?: {
      default?: React.CSSProperties;
      hover?: React.CSSProperties;
      pressed?: React.CSSProperties;
    };
    onMouseEnter?: (event: React.MouseEvent) => void;
    onMouseLeave?: (event: React.MouseEvent) => void;
    onClick?: (event: React.MouseEvent) => void;
    className?: string;
  }

  export interface MarkerProps {
    coordinates: [number, number];
    children?: React.ReactNode;
    onClick?: (event: React.MouseEvent) => void;
    style?: {
      default?: React.CSSProperties;
      hover?: React.CSSProperties;
      pressed?: React.CSSProperties;
    };
    className?: string;
  }

  export interface ZoomableGroupProps {
    center?: [number, number];
    zoom?: number;
    minZoom?: number;
    maxZoom?: number;
    translateExtent?: [[number, number], [number, number]];
    onMoveStart?: (position: { coordinates: [number, number]; zoom: number }) => void;
    onMove?: (position: { coordinates: [number, number]; zoom: number }) => void;
    onMoveEnd?: (position: { coordinates: [number, number]; zoom: number }) => void;
    children?: React.ReactNode;
  }

  export const ComposableMap: React.FC<ComposableMapProps>;
  export const Geographies: React.FC<GeographiesProps>;
  export const Geography: React.FC<GeographyProps>;
  export const Marker: React.FC<MarkerProps>;
  export const ZoomableGroup: React.FC<ZoomableGroupProps>;
}

declare module "d3-geo" {
  export function geoCentroid(geojson: object): [number, number];
}
