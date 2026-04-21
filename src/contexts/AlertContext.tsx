import React, { createContext, useContext, useRef, useCallback, ReactNode } from "react";
import { Toast, toast } from "@heroui/react";

export type AlertType = "default" | "accent" | "success" | "warning" | "danger";

export interface AlertMessage {
  id: string;
  title: ReactNode;
  description?: ReactNode;
  status?: AlertType;
  timeout?: number; // 0 means persistent
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
    };

    const existingToastKey = toastKeyRef.current.get(id);
    if (existingToastKey) {
      toast.close(existingToastKey);
      toastKeyRef.current.delete(id);
    }

    alertStateRef.current.set(id, nextMessage);

    const toastKey = toast(nextMessage.title, {
      description: nextMessage.description,
      variant: resolveVariant(nextMessage.status),
      timeout: nextMessage.timeout,
      isLoading: nextMessage.timeout === 0 && nextMessage.status === "accent",
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
    });
  }, [showAlert]);

  return (
    <AlertContext.Provider value={{ showAlert, updateAlert, closeAlert }}>
      {children}
      <Toast.Provider placement="bottom" className="bottom-6 left-1/2 -translate-x-1/2" maxVisibleToasts={4} />
    </AlertContext.Provider>
  );
}
