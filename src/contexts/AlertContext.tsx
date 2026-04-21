import React, { createContext, useContext, useRef, useCallback, ReactNode } from "react";
import { Toast, toast, Spinner, ProgressBar } from "@heroui/react";
import type { ButtonProps } from "@heroui/react";

export type AlertType = "default" | "accent" | "success" | "warning" | "danger";

export interface AlertMessage {
  id: string;
  title: ReactNode;
  description?: ReactNode;
  status?: AlertType;
  timeout?: number; // 0 means persistent
  actionProps?: ButtonProps;  // Action button configuration
  progress?: number;          // 0-100, for progress bar
  icon?: ReactNode;           // Custom icon override
}

interface AlertContextType {
  showAlert: (msg: Omit<AlertMessage, "id"> & { id?: string }) => string;
  updateAlert: (id: string, msg: Partial<Omit<AlertMessage, "id">>) => void;
  closeAlert: (id: string) => void;
}

const AlertContext = createContext<AlertContextType | null>(null);

function resolveVariant(status?: AlertType): "default" | "accent" | "success" | "warning" | "danger" {
  return status ?? "default";
}

// Icon mapping for each status
function getIndicatorForStatus(status?: AlertType, isLoading?: boolean): ReactNode {
  if (isLoading) return <Spinner size="sm" />;

  switch (status) {
    case "success":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" className="size-5 text-success">
          <path fillRule="evenodd" d="M2.25 12c0-5.385 4.365-9.75 9.75-9.75s9.75 4.365 9.75 9.75-4.365 9.75-9.75 9.75S2.25 17.385 2.25 12zm13.36-1.814a.75.75 0 10-1.22-.872l-3.236 4.53L9.53 12.22a.75.75 0 00-1.06 1.06l2.25 2.25a.75.75 0 001.14-.094l3.75-5.25z" clipRule="evenodd" />
        </svg>
      );
    case "warning":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" className="size-5 text-warning">
          <path fillRule="evenodd" d="M9.401 3.003c1.155-2 4.043-2 5.197 0l7.355 12.748c1.154 2 .29 4.5-1.973 4.5H3.266c-2.262 0-3.127-2.5-1.973-4.5L9.4 3.003zM12 8.25a.75.75 0 01.75.75v3.75a.75.75 0 01-1.5 0V9a.75.75 0 01.75-.75zm0 0s.384 2.4.75 2.4c.366 0 .75-2.4.75-2.4s-.384 2.4-.75 2.4c-.366 0-.75-2.4-.75-2.4z" clipRule="evenodd" />
        </svg>
      );
    case "danger":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" className="size-5 text-danger">
          <path fillRule="evenodd" d="M12 2.25c-5.385 0-9.75 4.365-9.75 9.75s4.365 9.75 9.75 9.75 9.75-4.365 9.75-9.75S17.385 2.25 12 2.25zm-1.72 6.97a.75.75 0 10-1.06 1.06L10.94 12l-1.72 1.97a.75.75 0 101.06 1.06L12 13.06l1.72 1.97a.75.75 0 101.06-1.06L13.06 12l1.72-1.97a.75.75 0 10-1.06-1.06L12 10.94 10.28 9.22z" clipRule="evenodd" />
        </svg>
      );
    case "accent":
      return <Spinner size="sm" />;
    default:
      return (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" className="size-5 text-default-600">
          <path fillRule="evenodd" d="M12 2.25c-5.385 0-9.75 4.365-9.75 9.75s4.365 9.75 9.75 9.75 9.75-4.365 9.75-9.75S17.385 2.25 12 2.25zm0 8.25a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0-5.25a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5a.75.75 0 01.75-.75z" clipRule="evenodd" />
        </svg>
      );
  }
}

export function useAlert() {
  const ctx = useContext(AlertContext);
  if (!ctx) throw new Error("useAlert must be used within AlertProvider");
  return ctx;
}

export function AlertProvider({ children }: { children: ReactNode }) {
  const alertStateRef = useRef<Map<string, AlertMessage>>(new Map());
  const toastKeyRef = useRef<Map<string, string>>(new Map());

  const closeAlert = useCallback((id: string) => {
    const toastKey = toastKeyRef.current.get(id);
    if (toastKey) {
      toast.close(toastKey);
      toastKeyRef.current.delete(id);
    }
    alertStateRef.current.delete(id);
  }, []);

  const showAlert = useCallback((msg: Omit<AlertMessage, "id"> & { id?: string }) => {
    const id = msg.id || Math.random().toString(36).slice(2, 10);
    const previous = alertStateRef.current.get(id);
    const nextMessage: AlertMessage = {
      id,
      title: msg.title ?? previous?.title ?? "通知",
      description: msg.description ?? previous?.description,
      status: msg.status ?? previous?.status ?? "default",
      timeout: msg.timeout ?? previous?.timeout ?? 4000,
      actionProps: msg.actionProps ?? previous?.actionProps,
      progress: msg.progress ?? previous?.progress,
      icon: msg.icon ?? previous?.icon,
    };

    const existingToastKey = toastKeyRef.current.get(id);
    if (existingToastKey) {
      toast.close(existingToastKey);
      toastKeyRef.current.delete(id);
    }

    alertStateRef.current.set(id, nextMessage);

    const isLoading = nextMessage.timeout === 0 && nextMessage.status === "accent";

    const toastKey = toast(nextMessage.title, {
      description: nextMessage.description,
      variant: resolveVariant(nextMessage.status),
      timeout: nextMessage.timeout,
      isLoading,
      indicator: nextMessage.icon ?? getIndicatorForStatus(nextMessage.status, isLoading),
      actionProps: nextMessage.actionProps,
      onClose: () => {
        if (toastKeyRef.current.get(id) === toastKey) {
          toastKeyRef.current.delete(id);
        }
      },
    });

    toastKeyRef.current.set(id, toastKey);
    return id;
  }, []);

  const updateAlert = useCallback((id: string, msg: Partial<Omit<AlertMessage, "id">>) => {
    const previous = alertStateRef.current.get(id);
    if (!previous && !msg.title) {
      return;
    }

    showAlert({
      id,
      title: msg.title ?? previous?.title ?? "通知",
      description: msg.description ?? previous?.description,
      status: msg.status ?? previous?.status,
      timeout: msg.timeout ?? previous?.timeout,
      actionProps: msg.actionProps ?? previous?.actionProps,
      progress: msg.progress ?? previous?.progress,
      icon: msg.icon ?? previous?.icon,
    });
  }, [showAlert]);

  return (
    <AlertContext.Provider value={{ showAlert, updateAlert, closeAlert }}>
      {children}
      <Toast.Provider placement="bottom end" className="bottom-4 right-4" maxVisibleToasts={5} />
    </AlertContext.Provider>
  );
}
